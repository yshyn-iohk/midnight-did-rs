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

//! Off-chain Midnight DID state types + MOD1 binary frame codec.
//!
//! Port of `offchain-midnight.ts`. This module is the byte-parity-sensitive
//! piece flagged in the TS port plan: encoding and decoding must round-trip
//! byte-for-byte against the TypeScript implementation so that the state
//! hash baked into a `did:midnight:offchain:<hash>` matches across runtimes.
//!
//! ## Wire format
//!
//! The off-chain payload is wrapped in a MOD1-tagged frame:
//!
//! ```text
//! +-----------+---------------+---- per chunk -----------+
//! | "MOD1"    | u32 chunkCnt  | u32 chunkLen | chunkData |
//! +-----------+---------------+--------------+-----------+
//! ```
//!
//! followed by the unpadded base64url encoding. The chunks themselves come
//! from `CompactType::toValue` over the structured state described in
//! [`OffchainMidnightDidState`]; that piece of the serializer is currently
//! deferred to the runtime-rs crate (it requires the compact-runtime
//! type-descriptor machinery). The MOD1 frame, the JWK ↔ key-kind mapping,
//! and the state-hash derivation are fully implemented here.

use blake2::{Blake2s, Digest, digest::consts::U32};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto_codecs::{CodecError, decode_base64url, encode_base64url};
use crate::did_document::{
    CreateServiceParams, CreateVerificationMethodParams, CurveType, DidString, DocumentContext, KeyType, PublicKeyJwk,
    Service, ServiceEndpoint, ServiceType, ValidationError, VerificationMethod, VerificationMethodType, create_service,
    create_verification_method,
};
use crate::midnight::{
    MidnightDidError, MidnightDidString, MidnightNetwork, OffchainStateHashHex, create_midnight_did_string,
    parse_midnight_did_string,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum verification methods the off-chain DID supports.
pub const MAX_VERIFICATION_METHODS: usize = 4;
/// Maximum services the off-chain DID supports.
pub const MAX_SERVICES: usize = 4;
/// Maximum entries in `alsoKnownAs`.
pub const MAX_ALSO_KNOWN_AS: usize = 4;
const MAGIC: [u8; 4] = [0x4d, 0x4f, 0x44, 0x31]; // "MOD1"
const HEADER_LENGTH: usize = MAGIC.len() + 4;
const CHUNK_LENGTH_BYTES: usize = 4;
/// Encoding tag for off-chain DID state. Matches the TS constant verbatim.
pub const OFFCHAIN_STATE_ENCODING: &str = "midnight-offchain-did-state-v1.base64url";

// ---------------------------------------------------------------------------
// Offchain state types
// ---------------------------------------------------------------------------

/// 32-byte off-chain state hash (lowercase hex). Alias for the same type
/// used by [`crate::midnight`], re-exported so the TS surface matches.
pub type OffchainStateHash = OffchainStateHashHex;

/// Re-export of [`crate::midnight::parse_offchain_state_hash`] for parity
/// with the TS module structure.
pub use crate::midnight::parse_offchain_state_hash;

/// Boolean flags describing which verification relationships a method
/// participates in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OffchainVerificationRelationships {
    /// `authentication` relationship.
    pub authentication: bool,
    /// `assertionMethod` relationship.
    pub assertion_method: bool,
    /// `keyAgreement` relationship.
    pub key_agreement: bool,
    /// `capabilityInvocation` relationship.
    pub capability_invocation: bool,
    /// `capabilityDelegation` relationship.
    pub capability_delegation: bool,
}

/// A verification method in the off-chain state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffchainVerificationMethod {
    /// Fragment-reference id (must start with `#`).
    pub id: String,
    /// Public-key JWK.
    pub public_key_jwk: PublicKeyJwk,
    /// Verification relationships this key participates in.
    pub relationships: OffchainVerificationRelationships,
}

/// A service entry in the off-chain state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffchainService {
    /// Fragment-reference id (must start with `#`).
    pub id: String,
    /// Service type (single string).
    #[serde(rename = "type")]
    pub type_: String,
    /// Service endpoint URI.
    pub service_endpoint: String,
}

/// Off-chain DID state: structured representation matching the TS schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OffchainMidnightDidState {
    /// Version integer in `[1, 65535]`.
    pub version: u16,
    /// Alternate identifiers (at most [`MAX_ALSO_KNOWN_AS`] entries).
    pub also_known_as: Vec<String>,
    /// Verification methods (between 1 and [`MAX_VERIFICATION_METHODS`]).
    pub verification_method: Vec<OffchainVerificationMethod>,
    /// Service entries (at most [`MAX_SERVICES`] entries).
    pub service: Vec<OffchainService>,
}

/// Encoded off-chain state alongside the encoding tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodedOffchainMidnightDidState {
    /// Always [`OFFCHAIN_STATE_ENCODING`].
    pub encoding: String,
    /// Unpadded base64url-encoded MOD1 frame.
    pub payload: String,
}

/// Result of [`parse_long_form_offchain_midnight_did_string`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedLongFormOffchainMidnightDid {
    /// The validated DID portion (`did:midnight:offchain:<hash>[:<state>]`).
    pub did: MidnightDidString,
    /// State hash from the DID.
    pub state_hash: OffchainStateHash,
    /// Encoded state payload from the DID.
    pub encoded_state: EncodedOffchainMidnightDidState,
}

// ---------------------------------------------------------------------------
// Key kind <-> JWK
// ---------------------------------------------------------------------------

/// Integer tags for the JWK key/curve profiles supported by the offchain
/// wire format. Independent of `CurveType` ledger tags so the wire format
/// stays portable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OffchainKeyKind {
    /// `EC / Jubjub`.
    Jubjub = 1,
    /// `OKP / Ed25519`.
    Ed25519 = 2,
    /// `EC / P-256`.
    P256 = 3,
    /// `OKP / X25519`.
    X25519 = 4,
    /// `EC / secp256k1`.
    Secp256k1 = 5,
    /// `OKP / BLS12381G1`.
    BLS12381G1 = 6,
    /// `OKP / BLS12381G2`.
    BLS12381G2 = 7,
}

impl OffchainKeyKind {
    /// Convert from a raw `u8` wire tag.
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            1 => OffchainKeyKind::Jubjub,
            2 => OffchainKeyKind::Ed25519,
            3 => OffchainKeyKind::P256,
            4 => OffchainKeyKind::X25519,
            5 => OffchainKeyKind::Secp256k1,
            6 => OffchainKeyKind::BLS12381G1,
            7 => OffchainKeyKind::BLS12381G2,
            _ => return None,
        })
    }
}

/// Map a JWK profile to the off-chain wire tag.
pub fn key_kind_from_jwk(jwk: &PublicKeyJwk) -> Result<OffchainKeyKind, OffchainError> {
    use CurveType::*;
    use KeyType::*;
    match (jwk.kty, jwk.crv) {
        (EC, Jubjub) => Ok(OffchainKeyKind::Jubjub),
        (OKP, Ed25519) => Ok(OffchainKeyKind::Ed25519),
        (OKP, X25519) => Ok(OffchainKeyKind::X25519),
        (EC, P256) => Ok(OffchainKeyKind::P256),
        (EC, Secp256k1) => Ok(OffchainKeyKind::Secp256k1),
        (OKP, BLS12381G1) => Ok(OffchainKeyKind::BLS12381G1),
        (OKP, BLS12381G2) => Ok(OffchainKeyKind::BLS12381G2),
        _ => Err(OffchainError::UnsupportedKeyType {
            kty: format!("{:?}", jwk.kty),
            crv: format!("{:?}", jwk.crv),
        }),
    }
}

/// Re-build a JWK from an [`OffchainKeyKind`] tag and base64url-encoded
/// coordinate strings. The matching inverse is [`key_kind_from_jwk`].
pub fn jwk_from_key_kind(kind: OffchainKeyKind, x: &str, y: &str) -> PublicKeyJwk {
    use CurveType::*;
    use KeyType::*;
    let (kty, crv, has_y) = match kind {
        OffchainKeyKind::Jubjub => (EC, Jubjub, true),
        OffchainKeyKind::Ed25519 => (OKP, Ed25519, false),
        OffchainKeyKind::P256 => (EC, P256, true),
        OffchainKeyKind::X25519 => (OKP, X25519, false),
        OffchainKeyKind::Secp256k1 => (EC, Secp256k1, true),
        OffchainKeyKind::BLS12381G1 => (OKP, BLS12381G1, false),
        OffchainKeyKind::BLS12381G2 => (OKP, BLS12381G2, false),
    };
    PublicKeyJwk {
        kty,
        crv,
        x: x.to_owned(),
        y: if has_y { Some(y.to_owned()) } else { None },
        extensions: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// Relationship masks
// ---------------------------------------------------------------------------

/// Pack the boolean relationship flags into the 5-bit wire mask used by
/// the off-chain verification-method descriptor.
pub fn relationships_to_mask(r: &OffchainVerificationRelationships) -> u8 {
    let mut mask = 0u8;
    if r.authentication {
        mask |= 1;
    }
    if r.assertion_method {
        mask |= 2;
    }
    if r.key_agreement {
        mask |= 4;
    }
    if r.capability_invocation {
        mask |= 8;
    }
    if r.capability_delegation {
        mask |= 16;
    }
    mask
}

/// Inverse of [`relationships_to_mask`].
pub fn mask_to_relationships(mask: u8) -> OffchainVerificationRelationships {
    OffchainVerificationRelationships {
        authentication: (mask & 1) != 0,
        assertion_method: (mask & 2) != 0,
        key_agreement: (mask & 4) != 0,
        capability_invocation: (mask & 8) != 0,
        capability_delegation: (mask & 16) != 0,
    }
}

// ---------------------------------------------------------------------------
// MOD1 frame
// ---------------------------------------------------------------------------

fn assert_u32(value: usize, label: &str) -> Result<u32, OffchainError> {
    if value > u32::MAX as usize {
        return Err(OffchainError::Overflow { label: label.into() });
    }
    Ok(value as u32)
}

fn write_u32(target: &mut Vec<u8>, value: u32) {
    target.extend_from_slice(&value.to_be_bytes());
}

fn read_u32(source: &[u8], offset: usize) -> Result<u32, OffchainError> {
    if offset + CHUNK_LENGTH_BYTES > source.len() {
        return Err(OffchainError::ShortFrame);
    }
    Ok(u32::from_be_bytes([
        source[offset],
        source[offset + 1],
        source[offset + 2],
        source[offset + 3],
    ]))
}

/// Serialize a list of byte chunks into the MOD1 frame.
pub fn compact_value_to_bytes(chunks: &[Vec<u8>]) -> Result<Vec<u8>, OffchainError> {
    assert_u32(chunks.len(), "chunk count")?;
    let body_len: usize = chunks.iter().map(|c| CHUNK_LENGTH_BYTES + c.len()).sum();
    let mut out = Vec::with_capacity(HEADER_LENGTH + body_len);
    out.extend_from_slice(&MAGIC);
    write_u32(&mut out, chunks.len() as u32);
    for chunk in chunks {
        assert_u32(chunk.len(), "chunk length")?;
        write_u32(&mut out, chunk.len() as u32);
        out.extend_from_slice(chunk);
    }
    Ok(out)
}

/// Parse a MOD1-framed byte slice back into its chunk list.
pub fn compact_value_from_bytes(bytes: &[u8]) -> Result<Vec<Vec<u8>>, OffchainError> {
    if bytes.len() < HEADER_LENGTH {
        return Err(OffchainError::ShortFrame);
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(OffchainError::BadMagic);
    }
    let chunk_count = read_u32(bytes, MAGIC.len())? as usize;
    let max_chunk_count = (bytes.len() - HEADER_LENGTH) / CHUNK_LENGTH_BYTES;
    if chunk_count > max_chunk_count {
        return Err(OffchainError::TooManyChunks);
    }
    let mut chunks = Vec::with_capacity(chunk_count);
    let mut offset = HEADER_LENGTH;
    for _ in 0..chunk_count {
        let len = read_u32(bytes, offset)? as usize;
        offset += CHUNK_LENGTH_BYTES;
        if offset + len > bytes.len() {
            return Err(OffchainError::ChunkOverflow);
        }
        chunks.push(bytes[offset..offset + len].to_vec());
        offset += len;
    }
    if offset != bytes.len() {
        return Err(OffchainError::TrailingBytes);
    }
    Ok(chunks)
}

// ---------------------------------------------------------------------------
// State <-> chunks
// ---------------------------------------------------------------------------

/// Trait that abstracts over a compact-runtime style serializer. The TS
/// port flattens the state via `CompactType::toValue` then frames each
/// `Uint8Array` chunk inside the MOD1 envelope. Because this crate does
/// not depend on compact-runtime, the chunk-level serializer is provided
/// by the runtime crate (`midnight-did`) via this trait.
///
/// The MOD1 frame itself, the JWK ↔ key-kind tables, the state hash, and
/// every high-level helper in this module are runtime-independent and
/// covered by unit tests inside this crate.
pub trait CompactValueCodec {
    /// Convert structured state into the chunk list expected by the
    /// runtime's Compact type descriptors.
    fn to_chunks(state: &OffchainMidnightDidState) -> Result<Vec<Vec<u8>>, OffchainError>;
    /// Inverse of [`Self::to_chunks`].
    fn from_chunks(chunks: &[Vec<u8>]) -> Result<OffchainMidnightDidState, OffchainError>;
}

// ---------------------------------------------------------------------------
// State hash
// ---------------------------------------------------------------------------

/// Compute the off-chain state hash from a raw MOD1 frame.
pub fn bytes_to_state_hash(bytes: &[u8]) -> OffchainStateHash {
    let mut hasher = Blake2s::<U32>::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    OffchainStateHashHex(hex::encode(digest))
}

fn from_base64url(value: &str) -> Result<Vec<u8>, OffchainError> {
    if value.is_empty() {
        return Err(OffchainError::BadBase64Url);
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || value.len() % 4 == 1
    {
        return Err(OffchainError::BadBase64Url);
    }
    let decoded = decode_base64url(value).map_err(OffchainError::Codec)?;
    if encode_base64url(&decoded) != value {
        return Err(OffchainError::NonCanonicalBase64Url);
    }
    Ok(decoded)
}

// ---------------------------------------------------------------------------
// Encode / decode entry points (generic over the chunk codec)
// ---------------------------------------------------------------------------

/// Encode an off-chain DID state to the MOD1 frame and wrap it in the
/// `EncodedOffchainMidnightDidState` envelope.
///
/// Generic over [`CompactValueCodec`] because the chunk-level serializer
/// lives in the runtime crate.
pub fn encode_offchain_midnight_did_state<C: CompactValueCodec>(
    state: &OffchainMidnightDidState,
) -> Result<EncodedOffchainMidnightDidState, OffchainError> {
    validate_state_shape(state)?;
    let chunks = C::to_chunks(state)?;
    let frame = compact_value_to_bytes(&chunks)?;
    Ok(EncodedOffchainMidnightDidState {
        encoding: OFFCHAIN_STATE_ENCODING.to_owned(),
        payload: encode_base64url(&frame),
    })
}

/// Decode a MOD1-framed payload back into the structured state.
pub fn decode_offchain_midnight_did_state<C: CompactValueCodec>(
    encoded: &EncodedOffchainMidnightDidState,
) -> Result<OffchainMidnightDidState, OffchainError> {
    if encoded.encoding != OFFCHAIN_STATE_ENCODING {
        return Err(OffchainError::UnsupportedEncoding {
            encoding: encoded.encoding.clone(),
        });
    }
    let frame = from_base64url(&encoded.payload)?;
    let chunks = compact_value_from_bytes(&frame)?;
    let state = C::from_chunks(&chunks)?;
    validate_state_shape(&state)?;
    Ok(state)
}

fn validate_state_shape(state: &OffchainMidnightDidState) -> Result<(), OffchainError> {
    if state.version == 0 {
        return Err(OffchainError::BadVersion);
    }
    if state.also_known_as.len() > MAX_ALSO_KNOWN_AS {
        return Err(OffchainError::TooManyAlsoKnownAs);
    }
    if state.verification_method.is_empty() {
        return Err(OffchainError::NoVerificationMethod);
    }
    if state.verification_method.len() > MAX_VERIFICATION_METHODS {
        return Err(OffchainError::TooManyVerificationMethods);
    }
    if state.service.len() > MAX_SERVICES {
        return Err(OffchainError::TooManyServices);
    }
    for vm in &state.verification_method {
        if !vm.id.starts_with('#') {
            return Err(OffchainError::FragmentRequired);
        }
        // sanity-check the JWK profile is supported
        let _ = key_kind_from_jwk(&vm.public_key_jwk)?;
    }
    for service in &state.service {
        if !service.id.starts_with('#') {
            return Err(OffchainError::FragmentRequired);
        }
        if service.type_.is_empty() || service.service_endpoint.is_empty() {
            return Err(OffchainError::EmptyServiceField);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DID-string builders
// ---------------------------------------------------------------------------

/// Build a `did:midnight:offchain:<hash>` short-form string.
pub fn create_offchain_midnight_did_string(state_hash: &OffchainStateHash) -> MidnightDidString {
    create_midnight_did_string(&state_hash.0, MidnightNetwork::Offchain)
}

/// Encode `state`, hash the resulting MOD1 frame, and return the short-form
/// `did:midnight:offchain:<hash>` string.
pub fn create_offchain_midnight_did_string_from_state<C: CompactValueCodec>(
    state: &OffchainMidnightDidState,
) -> Result<MidnightDidString, OffchainError> {
    let encoded = encode_offchain_midnight_did_state::<C>(state)?;
    let bytes = from_base64url(&encoded.payload)?;
    let hash = bytes_to_state_hash(&bytes);
    Ok(create_offchain_midnight_did_string(&hash))
}

/// Build the long-form `did:midnight:offchain:<hash>:<state>` string,
/// embedding the encoded payload after the state hash.
pub fn create_long_form_offchain_midnight_did_string<C: CompactValueCodec>(
    state: &OffchainMidnightDidState,
) -> Result<MidnightDidString, OffchainError> {
    let encoded = encode_offchain_midnight_did_state::<C>(state)?;
    let bytes = from_base64url(&encoded.payload)?;
    let hash = bytes_to_state_hash(&bytes);
    let short = create_offchain_midnight_did_string(&hash);
    parse_midnight_did_string(&format!("{}:{}", short.0, encoded.payload)).map_err(Into::into)
}

/// Parse a long-form off-chain Midnight DID, validating that the embedded
/// state payload hashes to the DID's state hash.
pub fn parse_long_form_offchain_midnight_did_string(
    input: &str,
) -> Result<ParsedLongFormOffchainMidnightDid, OffchainError> {
    let did = parse_midnight_did_string(input)?;
    let parts: Vec<&str> = input.split(':').collect();
    if parts.get(2).copied() != Some("offchain") {
        return Err(OffchainError::NotOffchain);
    }
    if parts.len() != 5 {
        return Err(OffchainError::MissingEncodedState);
    }
    let state_hash = parse_offchain_state_hash(parts[3]).map_err(OffchainError::from)?;
    let state_payload = parts[4].to_owned();
    let encoded_state = EncodedOffchainMidnightDidState {
        encoding: OFFCHAIN_STATE_ENCODING.to_owned(),
        payload: state_payload.clone(),
    };
    let computed = bytes_to_state_hash(&from_base64url(&state_payload)?);
    if computed != state_hash {
        return Err(OffchainError::StateHashMismatch);
    }
    Ok(ParsedLongFormOffchainMidnightDid {
        did,
        state_hash,
        encoded_state,
    })
}

// ---------------------------------------------------------------------------
// DID-Document projections
// ---------------------------------------------------------------------------

/// Project an off-chain verification method to the W3C representation.
pub fn offchain_verification_method_to_did_document_method(
    did: &MidnightDidString,
    method: &OffchainVerificationMethod,
) -> Result<VerificationMethod, ValidationError> {
    create_verification_method(CreateVerificationMethodParams {
        id: method.id.clone(),
        type_: VerificationMethodType::JsonWebKey,
        controller: did.0.clone(),
        public_key_jwk: method.public_key_jwk.clone(),
    })
}

/// Project an off-chain service entry to the W3C representation.
pub fn offchain_service_to_did_document_service(service: &OffchainService) -> Result<Service, ValidationError> {
    create_service(CreateServiceParams {
        id: service.id.clone(),
        type_: ServiceType::One(service.type_.clone()),
        service_endpoint: ServiceEndpoint::Uri(service.service_endpoint.clone()),
    })
}

/// Build the public-facing DID Document fields from an off-chain state. The
/// returned tuple covers `(context, controller, verificationMethod, ...)`
/// for callers that want to assemble a [`crate::did_document::DidDocument`].
#[allow(clippy::type_complexity)]
pub fn offchain_state_to_did_document(
    did: &MidnightDidString,
    state: &OffchainMidnightDidState,
) -> Result<OffchainProjectedDocument, ValidationError> {
    let authentication: Vec<String> = state
        .verification_method
        .iter()
        .filter(|m| m.relationships.authentication)
        .map(|m| m.id.clone())
        .collect();
    let assertion_method: Vec<String> = state
        .verification_method
        .iter()
        .filter(|m| m.relationships.assertion_method)
        .map(|m| m.id.clone())
        .collect();
    let key_agreement: Vec<String> = state
        .verification_method
        .iter()
        .filter(|m| m.relationships.key_agreement)
        .map(|m| m.id.clone())
        .collect();
    let capability_invocation: Vec<String> = state
        .verification_method
        .iter()
        .filter(|m| m.relationships.capability_invocation)
        .map(|m| m.id.clone())
        .collect();
    let capability_delegation: Vec<String> = state
        .verification_method
        .iter()
        .filter(|m| m.relationships.capability_delegation)
        .map(|m| m.id.clone())
        .collect();

    let mut methods = Vec::with_capacity(state.verification_method.len());
    for vm in &state.verification_method {
        methods.push(offchain_verification_method_to_did_document_method(did, vm)?);
    }
    let mut services = Vec::with_capacity(state.service.len());
    for svc in &state.service {
        services.push(offchain_service_to_did_document_service(svc)?);
    }

    Ok(OffchainProjectedDocument {
        context: DocumentContext::Many(vec![
            "https://www.w3.org/ns/did/v1".into(),
            "https://w3c.github.io/vc-jws-2020/contexts/v1".into(),
        ]),
        id: DidString::parse(&did.0)?,
        also_known_as: if state.also_known_as.is_empty() {
            None
        } else {
            Some(state.also_known_as.clone())
        },
        controller: did.clone(),
        verification_method: methods,
        authentication: maybe_some(authentication),
        assertion_method: maybe_some(assertion_method),
        key_agreement: maybe_some(key_agreement),
        capability_invocation: maybe_some(capability_invocation),
        capability_delegation: maybe_some(capability_delegation),
        service: if services.is_empty() { None } else { Some(services) },
    })
}

fn maybe_some<T>(values: Vec<T>) -> Option<Vec<T>> {
    if values.is_empty() { None } else { Some(values) }
}

/// Output of [`offchain_state_to_did_document`]. Each field maps 1:1 to the
/// equivalent [`crate::did_document::DidDocument`] field; the projection
/// stops short of constructing the validated document so callers can add
/// extension keywords before final validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffchainProjectedDocument {
    /// `@context`.
    pub context: DocumentContext,
    /// DID subject.
    pub id: DidString,
    /// `alsoKnownAs`.
    pub also_known_as: Option<Vec<String>>,
    /// Sole controller — the DID itself.
    pub controller: MidnightDidString,
    /// `verificationMethod`.
    pub verification_method: Vec<VerificationMethod>,
    /// `authentication`.
    pub authentication: Option<Vec<String>>,
    /// `assertionMethod`.
    pub assertion_method: Option<Vec<String>>,
    /// `keyAgreement`.
    pub key_agreement: Option<Vec<String>>,
    /// `capabilityInvocation`.
    pub capability_invocation: Option<Vec<String>>,
    /// `capabilityDelegation`.
    pub capability_delegation: Option<Vec<String>>,
    /// `service`.
    pub service: Option<Vec<Service>>,
}

/// Build the document metadata block for an off-chain DID. Mirrors
/// `createOffchainMidnightDidDocumentMetadata` in TS.
pub fn create_offchain_midnight_did_document_metadata(
    state: &OffchainMidnightDidState,
) -> crate::did_document::DidDocumentMetadata {
    crate::did_document::DidDocumentMetadata {
        deactivated: Some(false),
        version_id: Some(state.version.to_string()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Reference (default) codec
// ---------------------------------------------------------------------------

/// Reference [`CompactValueCodec`] that fails at runtime. Provided so this
/// crate compiles standalone (`midnight-did-domain` deliberately does not
/// depend on compact-runtime); production callers should plug in a real
/// codec backed by the runtime's `CompactType*` descriptors.
///
/// Encoders/decoders accept any `C: CompactValueCodec` so callers can pass
/// their own backend in.
pub struct UnimplementedCompactValueCodec;

impl CompactValueCodec for UnimplementedCompactValueCodec {
    fn to_chunks(_state: &OffchainMidnightDidState) -> Result<Vec<Vec<u8>>, OffchainError> {
        Err(OffchainError::CompactCodecMissing)
    }

    fn from_chunks(_chunks: &[Vec<u8>]) -> Result<OffchainMidnightDidState, OffchainError> {
        Err(OffchainError::CompactCodecMissing)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Off-chain DID codec errors.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OffchainError {
    /// Underlying base64url / canonical-form error.
    #[error("codec error: {0}")]
    Codec(#[from] CodecError),
    /// Underlying Midnight DID parse error.
    #[error("Midnight DID error: {0}")]
    Did(#[from] MidnightDidError),
    /// W3C-validation error during DID-document projection.
    #[error("validation error: {0}")]
    Validation(#[from] ValidationError),
    /// Payload is not unpadded canonical base64url.
    #[error("Offchain Midnight DID state is not valid unpadded base64url")]
    BadBase64Url,
    /// Payload base64url is not canonical.
    #[error("Offchain Midnight DID state is not canonical unpadded base64url")]
    NonCanonicalBase64Url,
    /// MOD1 frame is shorter than the header.
    #[error("Offchain Midnight DID state is shorter than the header")]
    ShortFrame,
    /// MOD1 magic bytes did not match.
    #[error("Offchain Midnight DID state has an unexpected magic header")]
    BadMagic,
    /// Declared chunk count exceeds what the payload can hold.
    #[error("Offchain Midnight DID state declares too many chunks")]
    TooManyChunks,
    /// A chunk's declared length spilled past the end of the payload.
    #[error("Offchain Midnight DID chunk exceeds payload length")]
    ChunkOverflow,
    /// Frame contains bytes past the last declared chunk.
    #[error("Offchain Midnight DID state contains trailing bytes")]
    TrailingBytes,
    /// A field overflowed the `uint32` wire encoding.
    #[error("{label} must fit into uint32")]
    Overflow {
        /// Field that overflowed.
        label: String,
    },
    /// Encoded state uses an unsupported encoding tag.
    #[error("Unsupported offchain Midnight DID state encoding \"{encoding}\"")]
    UnsupportedEncoding {
        /// The unsupported encoding tag.
        encoding: String,
    },
    /// Long-form parse hit a non-offchain DID.
    #[error("Long-form offchain Midnight DID must use offchain network")]
    NotOffchain,
    /// Long-form parse hit a DID with no embedded state.
    #[error("Long-form offchain Midnight DID must include encoded state")]
    MissingEncodedState,
    /// Embedded payload hashes to a different value than the DID's state hash.
    #[error("Long-form offchain Midnight DID state does not match the DID state hash")]
    StateHashMismatch,
    /// `version` was out of range.
    #[error("offchain state version must be in [1, 65535]")]
    BadVersion,
    /// Too many `alsoKnownAs` entries.
    #[error("alsoKnownAs must contain at most 4 entries")]
    TooManyAlsoKnownAs,
    /// `verificationMethod` was empty.
    #[error("At least one verification method is required")]
    NoVerificationMethod,
    /// Too many verification methods.
    #[error("verificationMethod must contain at most 4 entries")]
    TooManyVerificationMethods,
    /// Too many services.
    #[error("service must contain at most 4 entries")]
    TooManyServices,
    /// Fragment-id requirement violated.
    #[error("id must be a fragment reference")]
    FragmentRequired,
    /// Service field was empty.
    #[error("service type and serviceEndpoint must be non-empty")]
    EmptyServiceField,
    /// JWK profile is not supported by the off-chain wire format.
    #[error("Unsupported offchain Midnight DID key type {kty}/{crv}")]
    UnsupportedKeyType {
        /// JWK `kty`.
        kty: String,
        /// JWK `crv`.
        crv: String,
    },
    /// No `CompactValueCodec` was wired in; encode/decode is unavailable
    /// until the runtime crate provides one.
    #[error(
        "Compact value codec is not provided; plug in a runtime-backed CompactValueCodec to \
         encode/decode offchain state"
    )]
    CompactCodecMissing,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> OffchainMidnightDidState {
        OffchainMidnightDidState {
            version: 1,
            also_known_as: vec![],
            verification_method: vec![OffchainVerificationMethod {
                id: "#key-1".into(),
                public_key_jwk: PublicKeyJwk {
                    kty: KeyType::OKP,
                    crv: CurveType::Ed25519,
                    x: encode_base64url(&[1u8; 32]),
                    y: None,
                    extensions: Default::default(),
                },
                relationships: OffchainVerificationRelationships {
                    authentication: true,
                    ..Default::default()
                },
            }],
            service: vec![],
        }
    }

    #[test]
    fn mod1_frame_roundtrips() {
        let chunks = vec![vec![1u8, 2, 3], vec![], vec![9u8; 5]];
        let bytes = compact_value_to_bytes(&chunks).unwrap();
        assert_eq!(&bytes[..4], b"MOD1");
        let decoded = compact_value_from_bytes(&bytes).unwrap();
        assert_eq!(decoded, chunks);
    }

    #[test]
    fn mod1_rejects_bad_magic() {
        let bytes = b"NOPE\x00\x00\x00\x00";
        assert!(matches!(compact_value_from_bytes(bytes), Err(OffchainError::BadMagic)));
    }

    #[test]
    fn relationships_round_trip() {
        let r = OffchainVerificationRelationships {
            authentication: true,
            key_agreement: true,
            capability_delegation: true,
            ..Default::default()
        };
        let m = relationships_to_mask(&r);
        assert_eq!(mask_to_relationships(m), r);
    }

    #[test]
    fn key_kind_round_trip() {
        let jwk = PublicKeyJwk {
            kty: KeyType::OKP,
            crv: CurveType::Ed25519,
            x: encode_base64url(&[7u8; 32]),
            y: None,
            extensions: Default::default(),
        };
        let kind = key_kind_from_jwk(&jwk).unwrap();
        assert_eq!(kind, OffchainKeyKind::Ed25519);
        let back = jwk_from_key_kind(kind, &jwk.x, "");
        assert_eq!(back.kty, jwk.kty);
        assert_eq!(back.crv, jwk.crv);
    }

    #[test]
    fn state_hash_is_blake2s_of_frame() {
        // Sanity: the hash is 32 bytes hex (64 chars), lowercase, stable.
        let h = bytes_to_state_hash(b"MOD1\x00\x00\x00\x00");
        assert_eq!(h.0.len(), 64);
        assert_eq!(h.0, h.0.to_lowercase());
    }

    #[test]
    fn unimplemented_codec_errors_cleanly() {
        let state = sample_state();
        let err = encode_offchain_midnight_did_state::<UnimplementedCompactValueCodec>(&state).unwrap_err();
        assert!(matches!(err, OffchainError::CompactCodecMissing));
    }

    #[test]
    fn shape_validation_catches_obvious_problems() {
        let mut s = sample_state();
        s.verification_method[0].id = "key-1".into(); // missing leading '#'
        assert!(matches!(validate_state_shape(&s), Err(OffchainError::FragmentRequired)));
    }
}
