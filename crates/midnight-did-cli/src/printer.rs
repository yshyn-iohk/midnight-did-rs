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

//! JSON output helpers for the CLI demo.

use serde_json::Value;

/// Layout used when emitting one document to stdout.
#[derive(Debug, Clone, Copy)]
pub enum JsonLayout {
    /// Multi-line, two-space indented.
    Pretty,
    /// One line per document (machine-readable).
    Compact,
}

/// Render a JSON value into a string using the requested layout. Falls back
/// to a debug representation if serialization somehow fails — which should
/// never happen for the value shapes this binary produces.
pub fn render(value: &Value, layout: JsonLayout) -> String {
    let result = match layout {
        JsonLayout::Pretty => serde_json::to_string_pretty(value),
        JsonLayout::Compact => serde_json::to_string(value),
    };
    result.unwrap_or_else(|err| format!("<json serialization failed: {err}>"))
}

/// Print the standard `== Step N: <name> ==` header used between flow steps.
pub fn print_header(step_index: usize, step_name: &str) {
    println!("== Step {step_index}: {step_name} ==");
}

/// Print a JSON document followed by a blank line.
pub fn print_document(value: &Value, layout: JsonLayout) {
    println!("{}", render(value, layout));
    println!();
}
