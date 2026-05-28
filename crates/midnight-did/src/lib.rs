//! Native Rust implementation of the Midnight DID Method.
//!
//! Cycle 1: scaffold only. The `contract` module is populated by `just codegen`.

#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, clippy::all)]

/// Crate version reported by the build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_compiles() {
        assert!(!VERSION.is_empty());
    }
}
