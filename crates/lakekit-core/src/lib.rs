//! The I/O boundary shared by lakekit's table format crates. Format crates do no I/O of their own:
//! they read and write through a [`Host`] the application implements over its object store (an
//! `object_store` client natively, `fetch` or an R2 binding in WebAssembly), so they build for
//! `wasm32-unknown-unknown` and native targets alike.
//!
//! ```
//! use lakekit_core::{Host, MemoryHost};
//!
//! # block_on(async {
//! let host = MemoryHost::new(0);
//! host.put("memory://t/a", b"x".to_vec()).await?;
//! assert!(host.exists("memory://t/a").await?);
//! # Ok::<(), lakekit_core::Error>(())
//! # }).unwrap();
//! # // MemoryHost's futures are ready at once, so one poll runs them.
//! # fn block_on<F: std::future::Future>(f: F) -> F::Output {
//! #     let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
//! #     let mut f = std::pin::pin!(f);
//! #     loop { if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; } }
//! # }
//! ```

mod error;
mod host;
mod memory;
mod uuid;

pub use error::{Error, Result};
pub use host::{Host, Listed, Upload};
pub use memory::{MemoryHost, MemoryUpload};
pub use uuid::{uuid_v4, uuid_v7};
