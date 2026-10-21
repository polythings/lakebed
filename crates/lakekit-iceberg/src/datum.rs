//! Typed values, and Iceberg's single-value binary serialization, which manifests use for column
//! bounds and manifest lists for partition bounds.
//! <https://iceberg.apache.org/spec/#binary-single-value-serialization>

use std::cmp::Ordering;

use crate::types::PrimitiveType;
use crate::{Result, corrupt};

/// A value of a primitive type. Dates, times and timestamps keep Iceberg's integer encoding.
#[derive(Clone, Debug, PartialEq)]
pub enum Datum {
    /// `boolean`.
    Boolean(bool),
    /// `int`.
    Int(i32),
    /// `long`.
    Long(i64),
    /// `float`.
    Float(f32),
    /// `double`.
    Double(f64),
    /// `decimal(P,S)`: the unscaled value.
    Decimal {
        /// The value times 10^`scale`.
        unscaled: i128,
        /// Digits in all.
        precision: u8,
        /// Digits after the point.
        scale: u8,
    },
    /// `date`: days since 1970-01-01.
    Date(i32),
    /// `time`: microseconds since midnight.
    Time(i64),
    /// `timestamp`: microseconds since the epoch.
    Timestamp(i64),
    /// `timestamptz`: microseconds since the epoch, UTC.
    Timestamptz(i64),
    /// `timestamp_ns`: nanoseconds since the epoch.
    TimestampNs(i64),
    /// `timestamptz_ns`: nanoseconds since the epoch, UTC.
    TimestamptzNs(i64),
    /// `string`.
    String(String),
    /// `uuid`, big-endian.
    Uuid([u8; 16]),
    /// `fixed[L]`.
    Fixed(Vec<u8>),
    /// `binary`.
    Binary(Vec<u8>),
}

fn sized<const N: usize>(bytes: &[u8], ty: &PrimitiveType) -> Result<[u8; N]> {
    bytes.try_into().map_err(|_| corrupt(format!("a {ty} value of {} bytes", bytes.len())))
}

impl Datum {
    /// Decodes Iceberg's single-value serialization of a `ty` value. A `long` or `double` stored in
    /// 4 bytes was written before the column was promoted from `int` or `float`, and is widened.
    pub fn from_bytes(ty: &PrimitiveType, bytes: &[u8]) -> Result<Self> {
        Ok(match ty {
            PrimitiveType::Boolean => match bytes {
                [b] => Self::Boolean(*b != 0),
                _ => return Err(corrupt(format!("a boolean value of {} bytes", bytes.len()))),
            },
            PrimitiveType::Int => Self::Int(i32::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Long if bytes.len() == 4 => Self::Long(i32::from_le_bytes(sized(bytes, ty)?).into()),
            PrimitiveType::Long => Self::Long(i64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Float => Self::Float(f32::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Double if bytes.len() == 4 => Self::Double(f32::from_le_bytes(sized(bytes, ty)?).into()),
            PrimitiveType::Double => Self::Double(f64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Decimal { precision, scale } => {
                if bytes.is_empty() || bytes.len() > 16 {
                    return Err(corrupt(format!("a decimal value of {} bytes", bytes.len())));
                }
                Self::Decimal { unscaled: from_be(bytes), precision: *precision, scale: *scale }
            }
            PrimitiveType::Date => Self::Date(i32::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Time => Self::Time(i64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Timestamp => Self::Timestamp(i64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::Timestamptz => Self::Timestamptz(i64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::TimestampNs => Self::TimestampNs(i64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::TimestamptzNs => Self::TimestamptzNs(i64::from_le_bytes(sized(bytes, ty)?)),
            PrimitiveType::String => {
                Self::String(String::from_utf8(bytes.to_vec()).map_err(|_| corrupt("a string value that isn't UTF-8"))?)
            }
            PrimitiveType::Uuid => Self::Uuid(sized(bytes, ty)?),
            PrimitiveType::Fixed(_) => Self::Fixed(bytes.to_vec()),
            PrimitiveType::Binary => Self::Binary(bytes.to_vec()),
        })
    }

    /// Iceberg's single-value serialization of this value.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Boolean(b) => vec![u8::from(*b)],
            Self::Int(v) | Self::Date(v) => v.to_le_bytes().to_vec(),
            Self::Long(v)
            | Self::Time(v)
            | Self::Timestamp(v)
            | Self::Timestamptz(v)
            | Self::TimestampNs(v)
            | Self::TimestamptzNs(v) => v.to_le_bytes().to_vec(),
            Self::Float(v) => v.to_le_bytes().to_vec(),
            Self::Double(v) => v.to_le_bytes().to_vec(),
            Self::Decimal { unscaled, .. } => minimal_be(*unscaled),
            Self::String(s) => s.as_bytes().to_vec(),
            Self::Uuid(u) => u.to_vec(),
            Self::Fixed(b) | Self::Binary(b) => b.clone(),
        }
    }

    /// The value's type.
    pub fn primitive_type(&self) -> PrimitiveType {
        match self {
            Self::Boolean(_) => PrimitiveType::Boolean,
            Self::Int(_) => PrimitiveType::Int,
            Self::Long(_) => PrimitiveType::Long,
            Self::Float(_) => PrimitiveType::Float,
            Self::Double(_) => PrimitiveType::Double,
            Self::Decimal { precision, scale, .. } => PrimitiveType::Decimal { precision: *precision, scale: *scale },
            Self::Date(_) => PrimitiveType::Date,
            Self::Time(_) => PrimitiveType::Time,
            Self::Timestamp(_) => PrimitiveType::Timestamp,
            Self::Timestamptz(_) => PrimitiveType::Timestamptz,
            Self::TimestampNs(_) => PrimitiveType::TimestampNs,
            Self::TimestamptzNs(_) => PrimitiveType::TimestamptzNs,
            Self::String(_) => PrimitiveType::String,
            Self::Uuid(_) => PrimitiveType::Uuid,
            Self::Fixed(b) => PrimitiveType::Fixed(b.len() as u64),
            Self::Binary(_) => PrimitiveType::Binary,
        }
    }

    /// The value as an integer, for the integer-encoded types (`int`, `long`, dates, times, and
    /// timestamps).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(v) | Self::Date(v) => Some((*v).into()),
            Self::Long(v)
            | Self::Time(v)
            | Self::Timestamp(v)
            | Self::Timestamptz(v)
            | Self::TimestampNs(v)
            | Self::TimestamptzNs(v) => Some(*v),
            _ => None,
        }
    }

    /// Whether this is a float or double NaN.
    pub fn is_nan(&self) -> bool {
        match self {
            Self::Float(v) => v.is_nan(),
            Self::Double(v) => v.is_nan(),
            _ => false,
        }
    }
}

/// Values of the same type compare by value (strings and bytes lexicographically, as Iceberg
/// orders them); values of different types don't compare.
impl PartialOrd for Datum {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Boolean(a), Self::Boolean(b)) => a.partial_cmp(b),
            (Self::Int(a), Self::Int(b)) | (Self::Date(a), Self::Date(b)) => a.partial_cmp(b),
            (Self::Long(a), Self::Long(b))
            | (Self::Time(a), Self::Time(b))
            | (Self::Timestamp(a), Self::Timestamp(b))
            | (Self::Timestamptz(a), Self::Timestamptz(b))
            | (Self::TimestampNs(a), Self::TimestampNs(b))
            | (Self::TimestamptzNs(a), Self::TimestamptzNs(b)) => a.partial_cmp(b),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(b),
            (Self::Double(a), Self::Double(b)) => a.partial_cmp(b),
            (Self::Decimal { unscaled: a, scale: sa, .. }, Self::Decimal { unscaled: b, scale: sb, .. })
                if sa == sb =>
            {
                a.partial_cmp(b)
            }
            (Self::String(a), Self::String(b)) => a.partial_cmp(b),
            (Self::Uuid(a), Self::Uuid(b)) => a.partial_cmp(b),
            (Self::Fixed(a), Self::Fixed(b)) | (Self::Binary(a), Self::Binary(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

/// The shortest big-endian two's complement bytes of `v` (Iceberg's decimal encoding).
pub fn minimal_be(v: i128) -> Vec<u8> {
    let bytes = v.to_be_bytes();
    let mut start = 0;
    while start < bytes.len() - 1 {
        let (b, next) = (bytes[start], bytes[start + 1]);
        if (b == 0 && next & 0x80 == 0) || (b == 0xff && next & 0x80 != 0) {
            start += 1;
        } else {
            break;
        }
    }
    bytes[start..].to_vec()
}

/// A big-endian two's complement value of at most 16 bytes, as an integer.
pub fn from_be(bytes: &[u8]) -> i128 {
    let negative = bytes.first().is_some_and(|b| b & 0x80 != 0);
    let mut v: i128 = if negative { -1 } else { 0 };
    for &b in &bytes[bytes.len().saturating_sub(16)..] {
        v = (v << 8) | i128::from(b);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(d: &Datum) {
        assert_eq!(&Datum::from_bytes(&d.primitive_type(), &d.to_bytes()).unwrap(), d);
    }

    #[test]
    fn values_round_trip() {
        for d in [
            Datum::Boolean(true),
            Datum::Int(-7),
            Datum::Long(1 << 40),
            Datum::Float(0.5),
            Datum::Double(-1.25),
            Datum::Decimal { unscaled: -12_345, precision: 10, scale: 2 },
            Datum::Date(-1),
            Datum::Time(3_600_000_000),
            Datum::Timestamptz(1_700_000_000_000_000),
            Datum::TimestamptzNs(1_700_000_000_000_000_001),
            Datum::String("truck_1".into()),
            Datum::Uuid([7; 16]),
            Datum::Fixed(vec![1, 2, 3]),
            Datum::Binary(vec![]),
        ] {
            round_trip(&d);
        }
    }

    #[test]
    fn encodings_follow_the_spec() {
        assert_eq!(Datum::Int(7).to_bytes(), 7i32.to_le_bytes());
        assert_eq!(Datum::Boolean(false).to_bytes(), [0]);
        assert_eq!(Datum::Decimal { unscaled: 128, precision: 5, scale: 0 }.to_bytes(), [0x00, 0x80]);
        assert_eq!(Datum::String("ab".into()).to_bytes(), b"ab");
    }

    #[test]
    fn promoted_columns_widen() {
        assert_eq!(Datum::from_bytes(&PrimitiveType::Long, &(-3i32).to_le_bytes()).unwrap(), Datum::Long(-3));
        assert_eq!(Datum::from_bytes(&PrimitiveType::Double, &0.5f32.to_le_bytes()).unwrap(), Datum::Double(0.5));
    }

    #[test]
    fn malformed_values_are_errors() {
        assert!(Datum::from_bytes(&PrimitiveType::Long, &[1, 2, 3]).is_err());
        assert!(Datum::from_bytes(&PrimitiveType::Int, &[]).is_err());
        assert!(Datum::from_bytes(&PrimitiveType::Boolean, &[]).is_err());
        assert!(Datum::from_bytes(&PrimitiveType::String, &[0xff]).is_err());
        assert!(Datum::from_bytes(&PrimitiveType::Uuid, &[0; 15]).is_err());
        let decimal = PrimitiveType::Decimal { precision: 38, scale: 0 };
        assert!(Datum::from_bytes(&decimal, &[0; 17]).is_err());
    }

    #[test]
    fn only_same_types_compare() {
        assert!(Datum::Int(1) < Datum::Int(2));
        assert!(Datum::String("a".into()) < Datum::String("b".into()));
        assert_eq!(Datum::Int(1).partial_cmp(&Datum::Long(2)), None);
        assert_eq!(Datum::Double(f64::NAN).partial_cmp(&Datum::Double(1.0)), None);
    }

    #[test]
    fn decimal_bytes_are_minimal() {
        assert_eq!(minimal_be(0), [0]);
        assert_eq!(minimal_be(127), [0x7f]);
        assert_eq!(minimal_be(128), [0x00, 0x80]);
        assert_eq!(minimal_be(-1), [0xff]);
        assert_eq!(minimal_be(-129), [0xff, 0x7f]);
        assert_eq!(from_be(&minimal_be(-129)), -129);
        assert_eq!(from_be(&minimal_be(1 << 40)), 1 << 40);
    }
}
