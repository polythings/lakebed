//! The I/O boundary shared by lakekit's table format crates. Format crates do no I/O of their own:
//! they read and write through a [`Store`] the application implements over its objects (an
//! `object_store` client natively, `fetch` or an R2 binding in WebAssembly), and take the time and
//! randomness from an [`Env`], so they build for `wasm32-unknown-unknown` and native targets alike.
//!
//! - [`Store`]: get, ranged get, put, put-if-absent (the commit primitive), multipart upload, list,
//!   delete.
//! - [`Env`]: the time, randomness, and waiting. `TokioEnv` (feature `tokio`) is a native one.
//! - [`WithEnv`]: a store and an environment as one value, for APIs that need both.
//! - [`MemoryStore`]: both, in memory, for tests.
//!
//! ```
//! use lakekit_core::{MemoryStore, Store};
//!
//! # block_on(async {
//! let store = MemoryStore::new(0);
//! store.put("memory://t/a", b"x".to_vec()).await?;
//! assert!(store.exists("memory://t/a").await?);
//! # Ok::<(), lakekit_core::Error>(())
//! # }).unwrap();
//! # // MemoryStore's futures are ready at once, so one poll runs them.
//! # fn block_on<F: std::future::Future>(f: F) -> F::Output {
//! #     let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
//! #     let mut f = std::pin::pin!(f);
//! #     loop { if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; } }
//! # }
//! ```

mod env;
mod error;
mod ids;
mod memory;
mod store;

#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
pub use env::TokioEnv;
pub use env::{Env, WithEnv};
pub use error::{Error, Result};
pub use ids::{uuid_v4, uuid_v7};
pub use memory::{MemoryStore, MemoryUpload};
pub use store::{Listed, Store, Upload};
