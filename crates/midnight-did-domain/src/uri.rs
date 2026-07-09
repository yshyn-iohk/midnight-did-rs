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

//! RFC 3986 URI normalization rules.
//!
//! Port of `uri-normalization.ts`. The normalization is restricted to a
//! known set of network schemes (`http`, `https`, `ws`, `wss`); other
//! schemes are returned unchanged.

use serde_json::Value as JsonValue;

const KNOWN_SCHEMES: &[&str] = &["http", "https", "ws", "wss"];

fn is_default_port(scheme: &str, port: u16) -> bool {
    matches!(
        (scheme, port),
        ("http", 80) | ("https", 443) | ("ws", 80) | ("wss", 443)
    )
}

fn extract_scheme(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte == b':' {
            return Some(&value[..idx]);
        }
        if !(byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'.' | b'-')) {
            return None;
        }
    }
    None
}

/// Normalize a URI string per RFC 3986 § 6 (case folding, default-port
/// removal, etc.). Returns `value` unchanged when the scheme is unknown or
/// when the URL parser refuses to accept it (matching the TS port's
/// best-effort behavior).
#[must_use]
pub fn normalize_uri_string(value: &str) -> String {
    let Some(scheme) = extract_scheme(value) else {
        return value.to_owned();
    };
    let lowered_scheme = scheme.to_ascii_lowercase();
    if !KNOWN_SCHEMES.contains(&lowered_scheme.as_str()) {
        return value.to_owned();
    }
    let parsed = match url::Url::parse(value) {
        Ok(u) => u,
        Err(_) => return value.to_owned(),
    };
    let protocol = parsed.scheme().to_ascii_lowercase();
    let username = parsed.username();
    let password = parsed.password().unwrap_or("");
    let auth = if !username.is_empty() {
        if !password.is_empty() {
            format!("{username}:{password}@")
        } else {
            format!("{username}@")
        }
    } else {
        String::new()
    };
    let hostname = parsed.host_str().map(|h| h.to_ascii_lowercase()).unwrap_or_default();
    let port = match parsed.port() {
        Some(p) if !is_default_port(&protocol, p) => format!(":{p}"),
        _ => String::new(),
    };
    let had_trailing_slash = value.ends_with('/');
    let mut pathname = parsed.path().to_owned();
    if !had_trailing_slash && pathname == "/" {
        pathname.clear();
    }
    let search = parsed.query().map(|q| format!("?{q}")).unwrap_or_default();
    let hash = parsed.fragment().map(|f| format!("#{f}")).unwrap_or_default();
    format!("{protocol}://{auth}{hostname}{port}{pathname}{search}{hash}")
}

fn normalize_unknown(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::String(s) => JsonValue::String(normalize_uri_string(&s)),
        JsonValue::Array(items) => JsonValue::Array(items.into_iter().map(normalize_unknown).collect()),
        JsonValue::Object(map) => JsonValue::Object(map.into_iter().map(|(k, v)| (k, normalize_unknown(v))).collect()),
        other => other,
    }
}

/// Walk a generic JSON value and normalize every string it contains using
/// [`normalize_uri_string`]. Mirrors `normalizeServiceEndpointValue` in TS.
#[must_use]
pub fn normalize_service_endpoint_value(endpoint: JsonValue) -> JsonValue {
    normalize_unknown(endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_scheme_and_host() {
        assert_eq!(
            normalize_uri_string("HTTPS://Example.COM/Path"),
            "https://example.com/Path"
        );
    }

    #[test]
    fn strips_default_ports() {
        assert_eq!(normalize_uri_string("https://example.com:443/"), "https://example.com/");
    }

    #[test]
    fn leaves_unknown_schemes_alone() {
        assert_eq!(normalize_uri_string("did:example:1234"), "did:example:1234");
    }
}
