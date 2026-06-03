// This file is part of Compact.
// Copyright (C) 2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Thin shim around `uniffi::uniffi_bindgen_main` so foreign-language
//! bindings can be generated without installing the standalone
//! `uniffi-bindgen` crate.
//!
//! Usage:
//! ```text
//! cargo run --features cli --bin uniffi-bindgen -- \
//!     generate --library target/debug/libmidnight_did_uniffi.dylib \
//!     --language swift --out-dir generated-swift
//! ```

fn main() {
    uniffi::uniffi_bindgen_main()
}
