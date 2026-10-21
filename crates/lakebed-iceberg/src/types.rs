//! Iceberg's types and schemas, read from table metadata JSON.
//! <https://iceberg.apache.org/spec/#schemas-and-data-types>

use std::fmt;
use std::str::FromStr;

use serde_json::Value;

use crate::{Error, Result, corrupt};

/// A primitive type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    /// `boolean`.
    Boolean,
    /// `int`: 32-bit signed.
    Int,
    /// `long`: 64-bit signed.
    Long,
    /// `float`: 32-bit IEEE 754.
    Float,
    /// `double`: 64-bit IEEE 754.
    Double,
    /// `decimal(P,S)`.
    Decimal {
        /// Digits in all.
        precision: u8,
        /// Digits after the point.
        scale: u8,
    },
    /// `date`: days since 1970-01-01.
    Date,
    /// `time`: microseconds since midnight.
    Time,
    /// `timestamp`: microseconds since the epoch, without a time zone.
    Timestamp,
    /// `timestamptz`: microseconds since the epoch, UTC.
    Timestamptz,
    /// `timestamp_ns`: nanoseconds since the epoch, without a time zone (format v3).
    TimestampNs,
    /// `timestamptz_ns`: nanoseconds since the epoch, UTC (format v3).
    TimestamptzNs,
    /// `string`: UTF-8.
    String,
    /// `uuid`.
    Uuid,
    /// `fixed[L]`: `L` bytes.
    Fixed(u64),
    /// `binary`: any number of bytes.
    Binary,
}

impl FromStr for PrimitiveType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "boolean" => Self::Boolean,
            "int" => Self::Int,
            "long" => Self::Long,
            "float" => Self::Float,
            "double" => Self::Double,
            "date" => Self::Date,
            "time" => Self::Time,
            "timestamp" => Self::Timestamp,
            "timestamptz" => Self::Timestamptz,
            "timestamp_ns" => Self::TimestampNs,
            "timestamptz_ns" => Self::TimestamptzNs,
            "string" => Self::String,
            "uuid" => Self::Uuid,
            "binary" => Self::Binary,
            other => {
                if let Some(n) = other.strip_prefix("fixed[").and_then(|s| s.strip_suffix(']')) {
                    Self::Fixed(n.trim().parse().map_err(|_| corrupt(format!("type {other}")))?)
                } else if let Some(ps) = other.strip_prefix("decimal(").and_then(|s| s.strip_suffix(')')) {
                    let (p, s) = ps.split_once(',').ok_or_else(|| corrupt(format!("type {other}")))?;
                    let precision = p.trim().parse().map_err(|_| corrupt(format!("type {other}")))?;
                    let scale = s.trim().parse().map_err(|_| corrupt(format!("type {other}")))?;
                    Self::Decimal { precision, scale }
                } else {
                    return Err(Error::Unsupported(format!("type {other}")));
                }
            }
        })
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean => f.write_str("boolean"),
            Self::Int => f.write_str("int"),
            Self::Long => f.write_str("long"),
            Self::Float => f.write_str("float"),
            Self::Double => f.write_str("double"),
            Self::Decimal { precision, scale } => write!(f, "decimal({precision}, {scale})"),
            Self::Date => f.write_str("date"),
            Self::Time => f.write_str("time"),
            Self::Timestamp => f.write_str("timestamp"),
            Self::Timestamptz => f.write_str("timestamptz"),
            Self::TimestampNs => f.write_str("timestamp_ns"),
            Self::TimestamptzNs => f.write_str("timestamptz_ns"),
            Self::String => f.write_str("string"),
            Self::Uuid => f.write_str("uuid"),
            Self::Fixed(n) => write!(f, "fixed[{n}]"),
            Self::Binary => f.write_str("binary"),
        }
    }
}

/// Any type: a primitive, a nested type, or one this version doesn't model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// A primitive.
    Primitive(PrimitiveType),
    /// A struct of fields.
    Struct(Vec<NestedField>),
    /// A list.
    List {
        /// The element's field id.
        element_id: i32,
        /// Whether elements can't be null.
        element_required: bool,
        /// The element type.
        element: Box<Type>,
    },
    /// A map.
    Map {
        /// The key's field id.
        key_id: i32,
        /// The key type.
        key: Box<Type>,
        /// The value's field id.
        value_id: i32,
        /// Whether values can't be null.
        value_required: bool,
        /// The value type.
        value: Box<Type>,
    },
    /// A type this version doesn't model (format v3's `variant`, `geometry(…)`, `unknown`, …), by
    /// its name. Columns of such types can be listed but not decoded.
    Other(String),
}

impl Type {
    /// The type of a field's `type` JSON value.
    pub fn from_json(t: &Value) -> Result<Self> {
        if let Some(name) = t.as_str() {
            return Ok(match name.parse() {
                Ok(p) => Self::Primitive(p),
                Err(Error::Unsupported(_)) => Self::Other(name.to_string()),
                Err(e) => return Err(e),
            });
        }
        let id = |key: &str| {
            t.get(key)
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| corrupt(format!("type without {key}")))
        };
        let child = |key: &str| Self::from_json(t.get(key).ok_or_else(|| corrupt(format!("type without {key}")))?);
        let flag = |key: &str| t.get(key).and_then(Value::as_bool).unwrap_or(false);
        match t.get("type").and_then(Value::as_str) {
            Some("struct") => Ok(Self::Struct(fields(t)?)),
            Some("list") => Ok(Self::List {
                element_id: id("element-id")?,
                element_required: flag("element-required"),
                element: Box::new(child("element")?),
            }),
            Some("map") => Ok(Self::Map {
                key_id: id("key-id")?,
                key: Box::new(child("key")?),
                value_id: id("value-id")?,
                value_required: flag("value-required"),
                value: Box::new(child("value")?),
            }),
            other => Err(corrupt(format!("type {other:?}"))),
        }
    }

    /// The primitive type of the field with id `id` inside this type.
    fn primitive_by_id(&self, id: i32) -> Option<&PrimitiveType> {
        match self {
            Self::Struct(fields) => fields.iter().find_map(|f| f.primitive_by_id(id)),
            Self::List { element_id, element, .. } => {
                if *element_id == id {
                    element.as_primitive()
                } else {
                    element.primitive_by_id(id)
                }
            }
            Self::Map { key_id, key, value_id, value, .. } => {
                if *key_id == id {
                    key.as_primitive()
                } else if *value_id == id {
                    value.as_primitive()
                } else {
                    key.primitive_by_id(id).or_else(|| value.primitive_by_id(id))
                }
            }
            Self::Primitive(_) | Self::Other(_) => None,
        }
    }

    /// The primitive, if this is one.
    pub fn as_primitive(&self) -> Option<&PrimitiveType> {
        match self {
            Self::Primitive(p) => Some(p),
            _ => None,
        }
    }
}

/// A field of a struct or schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NestedField {
    /// Its id, unique in the schema.
    pub id: i32,
    /// Its name.
    pub name: String,
    /// Whether values can't be null.
    pub required: bool,
    /// Its type.
    pub ty: Type,
    /// Its documentation, if any.
    pub doc: Option<String>,
}

impl NestedField {
    fn from_json(f: &Value) -> Result<Self> {
        let name = f.get("name").and_then(Value::as_str).ok_or_else(|| corrupt("field without a name"))?;
        let id = f.get("id").and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok());
        Ok(Self {
            id: id.ok_or_else(|| corrupt(format!("field {name} without an id")))?,
            name: name.to_string(),
            required: f.get("required").and_then(Value::as_bool).unwrap_or(false),
            ty: Type::from_json(f.get("type").ok_or_else(|| corrupt(format!("field {name} without a type")))?)?,
            doc: f.get("doc").and_then(Value::as_str).map(str::to_string),
        })
    }

    fn primitive_by_id(&self, id: i32) -> Option<&PrimitiveType> {
        if self.id == id { self.ty.as_primitive() } else { self.ty.primitive_by_id(id) }
    }
}

fn fields(t: &Value) -> Result<Vec<NestedField>> {
    let fields = t.get("fields").and_then(Value::as_array).ok_or_else(|| corrupt("struct without fields"))?;
    fields.iter().map(NestedField::from_json).collect()
}

/// A table schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    /// Its id (0 for format v1 tables, which have one schema).
    pub schema_id: i32,
    /// Top-level fields.
    pub fields: Vec<NestedField>,
}

impl Schema {
    /// A schema from its metadata JSON (`{"schema-id": 0, "type": "struct", "fields": [...]}`).
    pub fn from_json(schema: &Value) -> Result<Self> {
        let id = schema.get("schema-id").and_then(Value::as_i64).unwrap_or(0);
        Ok(Self { schema_id: i32::try_from(id).map_err(|_| corrupt("schema id"))?, fields: fields(schema)? })
    }

    /// The top-level field named `name`.
    pub fn field_by_name(&self, name: &str) -> Option<&NestedField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The primitive type of the field (at any depth) with id `id`: the type of its bounds in
    /// manifests.
    pub fn primitive_by_id(&self, id: i32) -> Option<&PrimitiveType> {
        self.fields.iter().find_map(|f| f.primitive_by_id(id))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn type_names_round_trip() {
        for name in ["boolean", "decimal(10, 2)", "fixed[16]", "timestamptz_ns", "uuid", "binary"] {
            assert_eq!(name.parse::<PrimitiveType>().unwrap().to_string(), name);
        }
        assert_eq!("decimal(9,3)".parse::<PrimitiveType>().unwrap(), PrimitiveType::Decimal { precision: 9, scale: 3 });
        assert!(matches!("variant".parse::<PrimitiveType>(), Err(Error::Unsupported(_))));
        assert!(matches!("fixed[x]".parse::<PrimitiveType>(), Err(Error::Corrupt(_))));
    }

    #[test]
    fn schemas_find_nested_field_types() {
        let schema = Schema::from_json(&json!({
            "schema-id": 3, "type": "struct", "fields": [
                {"id": 1, "name": "time", "required": true, "type": "timestamptz"},
                {"id": 2, "name": "tags", "required": false, "type":
                    {"type": "map", "key-id": 4, "key": "string", "value-id": 5, "value-required": false, "value": "long"}},
                {"id": 3, "name": "points", "required": false, "type":
                    {"type": "list", "element-id": 6, "element-required": true, "element":
                        {"type": "struct", "fields": [{"id": 7, "name": "x", "required": true, "type": "double"}]}}},
                {"id": 8, "name": "v", "required": false, "type": "variant"}
            ]
        }))
        .unwrap();
        assert_eq!(schema.schema_id, 3);
        assert_eq!(schema.primitive_by_id(1), Some(&PrimitiveType::Timestamptz));
        assert_eq!(schema.primitive_by_id(5), Some(&PrimitiveType::Long));
        assert_eq!(schema.primitive_by_id(7), Some(&PrimitiveType::Double));
        assert_eq!(schema.primitive_by_id(6), None, "a struct element isn't primitive");
        assert_eq!(schema.field_by_name("v").unwrap().ty, Type::Other("variant".into()));
    }
}
