# midnight-did-rs

[![CI](https://github.com/yshyn-iohk/midnight-did-rs/actions/workflows/ci.yml/badge.svg?branch=cycle-1-bootstrap)](https://github.com/yshyn-iohk/midnight-did-rs/actions/workflows/ci.yml)

Midnight DID implementation in Rust.

Every PR is built on Linux + macOS (host target) **and** against
`wasm32-unknown-unknown` for the `midnight-did-domain` +
`midnight-did-api` crates. The wasm gate enforces the design claim
that the domain + api layers are runtime-agnostic and free of any
`midnight-*` deps that would block in-browser use (see
[`doc/architecture.md`](./doc/architecture.md) §6).
