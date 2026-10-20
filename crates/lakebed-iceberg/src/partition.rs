//! Partition specs and their transforms.
//! <https://iceberg.apache.org/spec/#partitioning>

use std::fmt;
use std::str::FromStr;

use serde_json::Value;

use crate::types::{PrimitiveType, Schema};
use crate::{Error, Result, corrupt};

/// Partition field ids start here when a format v1 spec leaves them out.
const FIRST_FIELD_ID: i32 = 1000;

/// How a partition value is derived from its source column.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Transform {
    /// The value itself.
    Identity,
    /// A hash of the value modulo N.
    Bucket(u32),
    /// The value truncated to width W.
    Truncate(u32),
    /// Years since 1970.
    Year,
    /// Months since 1970-01.
    Month,
    /// Days since 1970-01-01.
    Day,
    /// Hours since 1970-01-01 00:00.
    Hour,
    /// Always null.
    Void,
    /// A transform this version doesn't know, by name.
    Other(String),
}

impl FromStr for Transform {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let arg = |prefix: &str| {
            s.strip_prefix(prefix)
                .and_then(|r| r.strip_suffix(']'))
                .map(|n| n.trim().parse::<u32>().map_err(|_| corrupt(format!("transform {s}"))))
        };
        Ok(match s {
            "identity" => Self::Identity,
            "year" => Self::Year,
            "month" => Self::Month,
            "day" => Self::Day,
            "hour" => Self::Hour,
            "void" => Self::Void,
            _ => {
                if let Some(n) = arg("bucket[") {
                    Self::Bucket(n?)
                } else if let Some(w) = arg("truncate[") {
                    Self::Truncate(w?)
                } else {
                    Self::Other(s.to_string())
                }
            }
        })
    }
}

impl fmt::Display for Transform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity => f.write_str("identity"),
            Self::Bucket(n) => write!(f, "bucket[{n}]"),
            Self::Truncate(w) => write!(f, "truncate[{w}]"),
            Self::Year => f.write_str("year"),
            Self::Month => f.write_str("month"),
            Self::Day => f.write_str("day"),
            Self::Hour => f.write_str("hour"),
            Self::Void => f.write_str("void"),
            Self::Other(name) => f.write_str(name),
        }
    }
}

impl Transform {
    /// The type of the partition values this transform makes from a `source` column, as
    /// iceberg-rust types them (`day` gives a `date`); `None` for an unknown transform.
    pub fn result_type(&self, source: &PrimitiveType) -> Option<PrimitiveType> {
        match self {
            Self::Identity | Self::Truncate(_) | Self::Void => Some(source.clone()),
            Self::Bucket(_) | Self::Year | Self::Month | Self::Hour => Some(PrimitiveType::Int),
            Self::Day => Some(PrimitiveType::Date),
            Self::Other(_) => None,
        }
    }
}

/// A field of a partition spec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionField {
    /// The source column's field id.
    pub source_id: i32,
    /// This field's id (1000 and up).
    pub field_id: i32,
    /// Its name in manifests.
    pub name: String,
    /// How its values are derived.
    pub transform: Transform,
}

/// A partition spec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionSpec {
    /// Its id.
    pub spec_id: i32,
    /// Its fields, in the order of the partition tuple.
    pub fields: Vec<PartitionField>,
}

impl PartitionSpec {
    /// A spec from metadata JSON: `{"spec-id": 0, "fields": [...]}`.
    pub fn from_json(spec: &Value) -> Result<Self> {
        let id = spec.get("spec-id").and_then(Value::as_i64).unwrap_or(0);
        let fields = spec.get("fields").ok_or_else(|| corrupt("partition spec without fields"))?;
        Self::from_fields(i32::try_from(id).map_err(|_| corrupt("partition spec id"))?, fields)
    }

    /// A spec from its fields' JSON (a format v1 table's `partition-spec`, a manifest header's
    /// `partition-spec`).
    pub fn from_fields(spec_id: i32, fields: &Value) -> Result<Self> {
        let fields = fields.as_array().ok_or_else(|| corrupt("partition fields aren't a list"))?;
        let fields = fields
            .iter()
            .zip(FIRST_FIELD_ID..)
            .map(|(f, default_id)| {
                let int = |key: &str| f.get(key).and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok());
                Ok(PartitionField {
                    source_id: int("source-id").ok_or_else(|| corrupt("partition field without source-id"))?,
                    field_id: int("field-id").unwrap_or(default_id),
                    name: f
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| corrupt("partition field without a name"))?
                        .to_string(),
                    transform: f
                        .get("transform")
                        .and_then(Value::as_str)
                        .ok_or_else(|| corrupt("partition field without a transform"))?
                        .parse()?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self { spec_id, fields })
    }

    /// Whether files of this spec all share one partition.
    pub fn is_unpartitioned(&self) -> bool {
        self.fields.iter().all(|f| f.transform == Transform::Void)
    }

    /// Each field with the type of its values, given the table `schema`.
    pub fn partition_type(&self, schema: &Schema) -> Result<Vec<PartitionFieldType>> {
        self.fields
            .iter()
            .map(|f| {
                let source = schema
                    .primitive_by_id(f.source_id)
                    .ok_or_else(|| corrupt(format!("partition field {} has no primitive source column", f.name)))?;
                let ty = f
                    .transform
                    .result_type(source)
                    .ok_or_else(|| Error::Unsupported(format!("partition transform {}", f.transform)))?;
                Ok(PartitionFieldType { field_id: f.field_id, name: f.name.clone(), ty })
            })
            .collect()
    }
}

/// A partition field with the type of its values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionFieldType {
    /// The partition field's id.
    pub field_id: i32,
    /// Its name.
    pub name: String,
    /// The type of its values.
    pub ty: PrimitiveType,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn transforms_round_trip() {
        for name in ["identity", "bucket[16]", "truncate[4]", "year", "month", "day", "hour", "void"] {
            assert_eq!(name.parse::<Transform>().unwrap().to_string(), name);
        }
        assert_eq!("zorder".parse::<Transform>().unwrap(), Transform::Other("zorder".into()));
        assert!("bucket[x]".parse::<Transform>().is_err());
    }

    #[test]
    fn specs_type_their_fields() {
        let schema = Schema::from_json(&json!({"schema-id": 0, "fields": [
            {"id": 1, "name": "time", "required": true, "type": "timestamptz"},
            {"id": 2, "name": "device", "required": true, "type": "string"}
        ]}))
        .unwrap();
        // A v1 spec without field ids gets 1000 and up.
        let spec = PartitionSpec::from_fields(
            0,
            &json!([
                {"source-id": 1, "name": "time_day", "transform": "day"},
                {"source-id": 2, "name": "device_bucket", "transform": "bucket[8]"},
                {"source-id": 2, "name": "device", "transform": "identity"}
            ]),
        )
        .unwrap();
        let types = spec.partition_type(&schema).unwrap();
        let ids: Vec<i32> = types.iter().map(|t| t.field_id).collect();
        assert_eq!(ids, [1000, 1001, 1002]);
        let tys: Vec<&PrimitiveType> = types.iter().map(|t| &t.ty).collect();
        assert_eq!(tys, [&PrimitiveType::Date, &PrimitiveType::Int, &PrimitiveType::String]);
        assert!(!spec.is_unpartitioned());
        assert!(PartitionSpec::from_json(&json!({"spec-id": 1, "fields": []})).unwrap().is_unpartitioned());
    }
}
