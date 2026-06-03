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

//! UniFFI build-script hook.
//!
//! We use proc-macro mode (`#[uniffi::export]`, `#[derive(uniffi::Object)]`,
//! `#[derive(uniffi::Error)]`) rather than a UDL file. The
//! `uniffi::setup_scaffolding!()` macro inside `src/lib.rs` handles the
//! generated scaffolding, so this build script is intentionally minimal: it
//! exists so `uniffi = { features = ["build"] }` is built and the type-checks
//! for the `setup_scaffolding!()` macro have all symbols available at link
//! time on every supported target.

fn main() {
    // Re-run the build script only when the manifest or sources change.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/lib.rs");
}
