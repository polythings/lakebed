//! Reading Avro records by field name, as manifests and manifest lists are read: optional fields
//! are unions with null, and fields a format version lacks are absent.

use std::collections::HashMap;

use apache_avro::Reader;
use apache_avro::types::Value;

use crate::{Error, Result, corrupt};

pub(crate) fn error(e: impl std::fmt::Display) -> Error {
    corrupt(format!("avro: {e}"))
}

/// An Avro container file's header metadata.
pub(crate) type Header = HashMap<String, Vec<u8>>;

/// The records of an Avro container file, and its header metadata.
pub(crate) fn records(bytes: &[u8]) -> Result<(Vec<Value>, Header)> {
    let reader = Reader::new(bytes).map_err(error)?;
    let metadata = reader.user_metadata().clone();
    let records = reader.map(|v| v.map_err(error)).collect::<Result<_>>()?;
    Ok((records, metadata))
}

/// The value inside an optional field's union.
pub(crate) fn strip_union(v: &Value) -> &Value {
    match v {
        Value::Union(_, inner) => strip_union(inner),
        other => other,
    }
}

/// Field `name` of `record`, or `None` if it is absent or null.
pub(crate) fn field<'a>(record: &'a Value, name: &str) -> Option<&'a Value> {
    match strip_union(record) {
        Value::Record(fields) => {
            fields.iter().find(|(n, _)| n == name).map(|(_, v)| strip_union(v)).filter(|v| **v != Value::Null)
        }
        _ => None,
    }
}

/// The first of `names` that `record` has (fields renamed between format versions).
pub(crate) fn field_of<'a>(record: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|n| field(record, n))
}

pub(crate) fn long(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Long(n) => Some(*n),
        Value::Int(n) | Value::Date(n) => Some(i64::from(*n)),
        _ => None,
    }
}

pub(crate) fn int(v: Option<&Value>) -> Option<i32> {
    match v? {
        Value::Int(n) | Value::Date(n) => Some(*n),
        Value::Long(n) => i32::try_from(*n).ok(),
        _ => None,
    }
}

pub(crate) fn string(v: Option<&Value>) -> Option<&str> {
    match v? {
        Value::String(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn required<T>(v: Option<T>, what: &str) -> Result<T> {
    v.ok_or_else(|| corrupt(format!("manifest without {what}")))
}

/// An optional Avro value.
#[cfg(feature = "write")]
pub(crate) fn some(v: Value) -> Value {
    Value::Union(1, Box::new(v))
}

/// A null optional Avro value.
#[cfg(feature = "write")]
pub(crate) fn null() -> Value {
    Value::Union(0, Box::new(Value::Null))
}
