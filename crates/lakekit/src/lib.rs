//! Table formats read and written without I/O of their own, for WebAssembly and native: one crate
//! per format, re-exported here behind a feature each. The I/O boundary is [`core::Host`].
//!
//! | Feature | Crate |
//! |---|---|
//! | `iceberg` (default) | [`iceberg`]: Apache Iceberg (`lakekit-iceberg`) |
//! | `iceberg-write`, `iceberg-arrow`, `iceberg-parquet-stats` | its optional parts |

pub use lakekit_core as core;
#[cfg(feature = "iceberg")]
pub use lakekit_iceberg as iceberg;
