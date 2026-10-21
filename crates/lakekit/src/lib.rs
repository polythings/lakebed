//! Table formats read and written without I/O of their own, for WebAssembly and native: one crate
//! per format and per store adapter, re-exported here behind a feature each. The I/O boundary is
//! [`core::Store`], [`core::Clock`], and [`core::Random`].
//!
//! | Feature | Crate |
//! |---|---|
//! | `iceberg` (default) | [`iceberg`]: Apache Iceberg (`lakekit-iceberg`) |
//! | `iceberg-write`, `iceberg-arrow`, `iceberg-parquet-stats` | its optional parts |
//! | `object-store` | `object_store`: a store over an `object_store` client (`lakekit-object-store`) |
//! | `tokio`, `getrandom` | `core::TokioClock` and `core::OsRandom`, native ones |

pub use lakekit_core as core;
#[cfg(feature = "iceberg")]
pub use lakekit_iceberg as iceberg;
#[cfg(feature = "object-store")]
pub use lakekit_object_store as object_store;
