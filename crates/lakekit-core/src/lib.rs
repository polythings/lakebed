//! The I/O boundary shared by lakekit's table format crates. Format crates do no I/O of their own:
//! they read and write through a [`Store`] the application supplies (an `object_store` client
//! natively, `fetch` or an R2 binding in WebAssembly), and take the time from a [`Clock`] and random
//! numbers from a [`Random`], so they build for `wasm32-unknown-unknown` and native targets alike.
//! The three are [loko](https://github.com/polythings/loko)'s resources, re-exported here.
//!
//! - [`Store`]: get, ranged get, put, put-if-absent (the commit primitive), multipart upload, list,
//!   delete. `loko-object-store` implements it over `object_store`.
//! - [`Clock`]: the time and a timer. `TokioClock` (feature `tokio`) is a native one.
//! - [`Random`]: random numbers. `OsRandom` (feature `getrandom`) is a native one.
//! - [`Resources`]: a store, a clock, and a random source as one value, for APIs that need all three;
//!   [`MemoryResources`] is all three in memory, for tests.
//! - [`Error`]: one error type for the format crates, converted from the store's.
//!
//! ```
//! use lakekit_core::{MemoryStore, Store};
//!
//! # block_on(async {
//! let store = MemoryStore::new();
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

mod error;
mod ids;
mod resources;

pub use error::{Error, Result};
pub use ids::{uuid_v4, uuid_v7};
#[cfg(all(feature = "tokio", not(target_arch = "wasm32")))]
pub use loko_clock::TokioClock;
pub use loko_clock::{Clock, HasClock, TestClock};
#[cfg(all(feature = "getrandom", not(all(target_arch = "wasm32", target_os = "unknown"))))]
pub use loko_random::OsRandom;
pub use loko_random::{CryptoRng, HasRandom, Random, Rng, SeededRandom, SeededRng};
pub use loko_store::{Error as StoreError, HasStore, Listed, MemoryStore, MemoryUpload, Store, Upload};
pub use resources::{MemoryResources, Resources};
