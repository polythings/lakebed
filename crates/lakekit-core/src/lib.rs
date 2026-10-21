//! The I/O boundary shared by lakekit's table format crates. Format crates do no I/O of their own:
//! they read and write through a [`Store`] the application implements over its objects (an
//! `object_store` client natively, `fetch` or an R2 binding in WebAssembly), and take the time from
//! a [`Clock`] and random numbers from a [`Random`], so they build for `wasm32-unknown-unknown` and
//! native targets alike.
//!
//! - [`Store`]: get, ranged get, put, put-if-absent (the commit primitive), multipart upload, list,
//!   delete.
//! - [`Clock`]: the time and a timer. `TokioClock` (feature `tokio`) is a native one.
//! - [`Random`]: random numbers. `OsRandom` (feature `getrandom`) is a native one.
//! - [`Services`]: a store, a clock, and a random source as one value, for APIs that need all three.
//! - [`MemoryStore`]: all three, in memory, for tests.
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

mod clock;
mod error;
mod ids;
mod memory;
mod random;
mod services;
mod store;

pub use clock::Clock;
#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
pub use clock::TokioClock;
pub use error::{Error, Result};
pub use ids::{uuid_v4, uuid_v7};
pub use memory::{MemoryStore, MemoryUpload};
#[cfg(all(feature = "getrandom", not(target_arch = "wasm32")))]
pub use random::OsRandom;
pub use random::Random;
pub use services::Services;
pub use store::{Listed, Store, Upload};
