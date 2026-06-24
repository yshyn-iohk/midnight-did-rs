//! Native Rust implementation of the Midnight DID Method.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod backend;
pub mod contract;

pub use backend::{Backend, BackendError, BuiltTx, FinalizedTxData, LiveBackend, RecordingBackend, ResolverBackend};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_compiles() {
        assert!(!VERSION.is_empty());
    }
}
