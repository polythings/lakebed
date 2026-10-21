//! Iceberg schemas as Arrow schemas (feature `arrow`), with each field's Iceberg id in its
//! metadata under [`FIELD_ID_KEY`], as Parquet writers and iceberg-rust's `schema_to_arrow_schema`
//! carry it.

use std::collections::HashMap;
use std::sync::Arc;

use arrow_schema::{DataType, Field, Fields, TimeUnit};

use crate::types::{NestedField, PrimitiveType, Schema, Type};
use crate::{Error, Result};

/// The Arrow field metadata key carrying an Iceberg field id.
pub const FIELD_ID_KEY: &str = "PARQUET:field_id";

/// The time zone of `timestamptz` columns in Arrow.
pub const UTC: &str = "+00:00";

/// `schema` as an Arrow schema.
pub fn schema_to_arrow(schema: &Schema) -> Result<arrow_schema::Schema> {
    Ok(arrow_schema::Schema::new(fields(&schema.fields)?))
}

fn fields(fields: &[NestedField]) -> Result<Vec<Field>> {
    fields.iter().map(|f| field(&f.name, f.id, !f.required, &f.ty)).collect()
}

fn field(name: &str, id: i32, nullable: bool, ty: &Type) -> Result<Field> {
    let metadata = HashMap::from([(FIELD_ID_KEY.to_string(), id.to_string())]);
    Ok(Field::new(name, data_type(ty)?, nullable).with_metadata(metadata))
}

/// The Arrow type of an Iceberg type.
pub fn data_type(ty: &Type) -> Result<DataType> {
    Ok(match ty {
        Type::Primitive(p) => primitive(p),
        Type::Struct(f) => DataType::Struct(Fields::from(fields(f)?)),
        Type::List { element_id, element_required, element } => {
            DataType::List(Arc::new(field("element", *element_id, !element_required, element)?))
        }
        Type::Map { key_id, key, value_id, value_required, value } => {
            let entries = Field::new(
                "key_value",
                DataType::Struct(Fields::from(vec![
                    field("key", *key_id, false, key)?,
                    field("value", *value_id, !value_required, value)?,
                ])),
                false,
            );
            DataType::Map(Arc::new(entries), false)
        }
        Type::Other(name) => return Err(Error::Unsupported(format!("{name} columns in Arrow"))),
    })
}

fn primitive(p: &PrimitiveType) -> DataType {
    match p {
        PrimitiveType::Boolean => DataType::Boolean,
        PrimitiveType::Int => DataType::Int32,
        PrimitiveType::Long => DataType::Int64,
        PrimitiveType::Float => DataType::Float32,
        PrimitiveType::Double => DataType::Float64,
        PrimitiveType::Decimal { precision, scale } => {
            DataType::Decimal128(*precision, i8::try_from(*scale).unwrap_or(i8::MAX))
        }
        PrimitiveType::Date => DataType::Date32,
        PrimitiveType::Time => DataType::Time64(TimeUnit::Microsecond),
        PrimitiveType::Timestamp => DataType::Timestamp(TimeUnit::Microsecond, None),
        PrimitiveType::Timestamptz => DataType::Timestamp(TimeUnit::Microsecond, Some(UTC.into())),
        PrimitiveType::TimestampNs => DataType::Timestamp(TimeUnit::Nanosecond, None),
        PrimitiveType::TimestamptzNs => DataType::Timestamp(TimeUnit::Nanosecond, Some(UTC.into())),
        PrimitiveType::String => DataType::Utf8,
        PrimitiveType::Uuid => DataType::FixedSizeBinary(16),
        PrimitiveType::Fixed(n) => DataType::FixedSizeBinary(i32::try_from(*n).unwrap_or(i32::MAX)),
        PrimitiveType::Binary => DataType::LargeBinary,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn fields_carry_ids() {
        let schema = Schema::from_json(&json!({"fields": [
            {"id": 1, "name": "time", "required": true, "type": "timestamptz"},
            {"id": 2, "name": "tags", "required": false, "type":
                {"type": "map", "key-id": 3, "key": "string", "value-id": 4, "value-required": true, "value": "decimal(10,2)"}}
        ]}))
        .unwrap();
        let arrow = schema_to_arrow(&schema).unwrap();
        let time = arrow.field(0);
        assert_eq!(time.data_type(), &DataType::Timestamp(TimeUnit::Microsecond, Some(UTC.into())));
        assert!(!time.is_nullable());
        assert_eq!(time.metadata()[FIELD_ID_KEY], "1");
        let DataType::Map(entries, _) = arrow.field(1).data_type() else { panic!("not a map") };
        let DataType::Struct(kv) = entries.data_type() else { panic!("not a struct") };
        assert_eq!(kv[1].metadata()[FIELD_ID_KEY], "4");
        assert_eq!(kv[1].data_type(), &DataType::Decimal128(10, 2));
    }
}
