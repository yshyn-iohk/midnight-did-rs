//! I/O substrate abstraction for the Midnight DID contract.
//!
//! The [`Backend`] trait is the three-method seam between
//! [`crate::Contract<B>`]'s circuit-call surface and whichever stack is
//! actually shuttling bytes to a Midnight node. Production code wraps a
//! wallet SDK + proof server + indexer in [`LiveBackend`]; api-layer
//! tests use [`RecordingBackend`] (in-memory, records every submit);
//! the resolver consumer uses [`ResolverBackend`] (read-only snapshot).
//!
//! ## v0.4.0 — Path 2 typed-envelope strategy
//!
//! R2-2 (ADR 0008) lands the typed [`crate::contract_call::DidContractCall`]
//! envelope on top of [`Backend::submit_tx`]: `Contract<B>` serialises each
//! circuit invocation into [`BuiltTx::bytes`] via
//! [`crate::contract_call::DidContractCall::encode`], and the recording
//! backend decodes the envelope back into typed call records via
//! [`Self::recorded_calls`]. [`Backend::read_snapshot`] is a parallel read
//! path that bypasses the submit/encode round-trip and hands callers a
//! plain-data [`DidLedgerSnapshot`] directly.
//!
//! See `doc/adr/0008-contract-abstraction-reform.md` for the full
//! rationale; the original `BuiltTx`-opaque scaffold landed in R2-1 and is
//! preserved here so `LiveBackend` stays implementable once the wallet
//! bridge is in place — only the recording backend cares about the JSON
//! envelope shape.

use std::fmt;
use std::sync::Mutex;

use async_trait::async_trait;
use compact_runtime::{empty_charged_state, ChargedState, DefaultDB};

// Re-export the upstream raw-state types under the backend module so
// downstream consumers (api-layer tests, future custom backends) can
// implement `Backend` without taking a direct `compact-runtime` dep.
pub use compact_runtime::{ChargedState as RawChargedState, DefaultDB as RawDb};

use crate::contract_call::{DidContractCall, DidLedgerSnapshot};

/// A transaction built and proven by the upstream wallet + proof stack,
/// ready for submission via [`Backend::submit_tx`].
///
/// v0.4.0 (Path 2): the bytes are produced by
/// [`crate::contract_call::DidContractCall::encode`] when the active backend
/// is `RecordingBackend`. Once the wallet/proof bridge lands, `LiveBackend`
/// will populate this with the upstream Midnight transaction shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuiltTx {
    /// Opaque transaction bytes. The active backend owns the encoding;
    /// recording backends use [`DidContractCall::encode`] /
    /// [`DidContractCall::decode`].
    pub bytes: Vec<u8>,
}

/// Finalisation data for a submitted transaction.
///
/// Mirrors the shape of the legacy `midnight_did_api::contract::FinalizedTxData`.
/// Once R2-2 lands the api crate re-exports this type so the duplicate
/// definition is retired.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinalizedTxData {
    /// Transaction hash (hex). Synthesised by [`RecordingBackend::submit_tx`]
    /// from a blake2b of the envelope bytes; empty until [`LiveBackend`]
    /// is wired.
    pub tx_hash: String,
    /// Block height the transaction was included in. Synthesised by
    /// [`RecordingBackend::submit_tx`] from the recorded-call count.
    pub block_height: u64,
}

/// Errors raised by a [`Backend`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// Network / RPC failure talking to the Midnight node or indexer.
    Network(String),
    /// The on-chain state, or the [`DidContractCall`] envelope, could not
    /// be decoded into the expected shape.
    Decode(String),
    /// The backend is read-only — used by [`ResolverBackend`] to reject
    /// any [`Backend::submit_tx`] call.
    ReadOnly,
    /// Catch-all for backend-specific failure modes not yet modelled.
    Other(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(m) => write!(f, "backend network failure: {m}"),
            Self::Decode(m) => write!(f, "backend decode failure: {m}"),
            Self::ReadOnly => write!(f, "backend is read-only"),
            Self::Other(m) => write!(f, "backend error: {m}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Substrate-agnostic I/O abstraction for the DID contract.
///
/// Three methods: submit a (proven, signed) envelope; read the raw
/// [`ChargedState`] (used by the future wallet bridge); read a decoded
/// [`DidLedgerSnapshot`] (used today by `Contract<B>::read_snapshot`).
#[async_trait]
pub trait Backend: Send + Sync {
    /// Submit a built transaction envelope and return its finalisation data.
    async fn submit_tx(&self, tx: BuiltTx) -> Result<FinalizedTxData, BackendError>;

    /// Read the raw on-chain [`ChargedState`].
    ///
    /// Kept for the future wallet/proof bridge that will decode it
    /// alongside upstream `Ledger::<DefaultDB>::new(...)`. `Contract<B>`
    /// callers should prefer [`Self::read_snapshot`].
    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError>;

    /// Read a plain-data [`DidLedgerSnapshot`].
    ///
    /// This is the read path `Contract<B>::read_snapshot` drives. For
    /// [`LiveBackend`] this will eventually wire the `Ledger -> DidLedgerSnapshot`
    /// mapper; for [`RecordingBackend`] and [`ResolverBackend`] the snapshot
    /// is stored verbatim by the test / consumer.
    async fn read_snapshot(&self) -> Result<DidLedgerSnapshot, BackendError>;
}

// ─────────────────────────────────────────────────────────────────────
// LiveBackend
// ─────────────────────────────────────────────────────────────────────

/// Production backend: wallet SDK + proof server + indexer.
///
/// # TODO
///
/// v0.4.0 ships this as a placeholder whose methods panic via [`todo!`].
/// The next cycle wires the real upstream stack:
///
/// - `wallet_sdk` — drives transaction construction + signing.
/// - `proof_server` — produces the halo2 proofs `submit_tx` carries.
/// - `indexer` — services `read_state` / `read_snapshot` via the public-data provider.
///
/// Tracked in ADR 0008 (R2 contract abstraction reform).
#[derive(Debug, Default)]
pub struct LiveBackend {
    /// Wallet SDK handle. `()` until the wallet bridge lands.
    pub wallet_sdk: (),
    /// Proof-server client handle. `()` until the wallet bridge lands.
    pub proof_server: (),
    /// Indexer / public-data-provider client. `()` until the wallet bridge lands.
    pub indexer: (),
}

impl LiveBackend {
    /// Construct a placeholder [`LiveBackend`].
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Backend for LiveBackend {
    async fn submit_tx(&self, _tx: BuiltTx) -> Result<FinalizedTxData, BackendError> {
        todo!("LiveBackend: wire wallet+proof+indexer")
    }

    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError> {
        todo!("LiveBackend: wire wallet+proof+indexer")
    }

    async fn read_snapshot(&self) -> Result<DidLedgerSnapshot, BackendError> {
        todo!("LiveBackend: wire the Ledger -> DidLedgerSnapshot mapper")
    }
}

// ─────────────────────────────────────────────────────────────────────
// RecordingBackend
// ─────────────────────────────────────────────────────────────────────

/// In-memory mock backend used by api-layer tests.
///
/// [`Backend::submit_tx`] decodes the envelope into a [`DidContractCall`]
/// and pushes it onto an internal call list ([`Self::recorded_calls`]).
/// [`Backend::read_state`] returns a clone of the stored
/// [`ChargedState`] (defaults to [`empty_charged_state`]).
/// [`Backend::read_snapshot`] returns a clone of the stored
/// [`DidLedgerSnapshot`] and records a synthetic
/// [`DidContractCall::ReadLedger`] entry to preserve the legacy
/// `RecordedCall::ReadLedger` test-sequencing semantics.
pub struct RecordingBackend {
    calls: Mutex<Vec<DidContractCall>>,
    state: Mutex<ChargedState<DefaultDB>>,
    snapshot: Mutex<DidLedgerSnapshot>,
}

impl fmt::Debug for RecordingBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let calls_len = self.calls.lock().map(|v| v.len()).unwrap_or(0);
        f.debug_struct("RecordingBackend")
            .field("recorded_call_count", &calls_len)
            .field("state", &"<ChargedState<DefaultDB>>")
            .field("snapshot", &"<DidLedgerSnapshot>")
            .finish()
    }
}

impl Default for RecordingBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingBackend {
    /// Construct a fresh [`RecordingBackend`] with no recorded calls and
    /// an empty [`ChargedState`] + default [`DidLedgerSnapshot`].
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            state: Mutex::new(empty_charged_state::<DefaultDB>()),
            snapshot: Mutex::new(DidLedgerSnapshot::default()),
        }
    }

    /// Construct a [`RecordingBackend`] seeded with a specific [`ChargedState`].
    pub fn with_state(state: ChargedState<DefaultDB>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            state: Mutex::new(state),
            snapshot: Mutex::new(DidLedgerSnapshot::default()),
        }
    }

    /// Construct a [`RecordingBackend`] seeded with a specific [`DidLedgerSnapshot`].
    pub fn with_snapshot(snapshot: DidLedgerSnapshot) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            state: Mutex::new(empty_charged_state::<DefaultDB>()),
            snapshot: Mutex::new(snapshot),
        }
    }

    /// Snapshot of every decoded [`DidContractCall`] in submission order
    /// (including synthetic [`DidContractCall::ReadLedger`] entries pushed
    /// by [`Self::read_snapshot`]).
    pub fn recorded_calls(&self) -> Vec<DidContractCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Replace the [`ChargedState`] returned by [`Backend::read_state`].
    pub fn set_state(&self, state: ChargedState<DefaultDB>) {
        *self.state.lock().unwrap() = state;
    }

    /// Replace the snapshot returned by [`Backend::read_snapshot`].
    pub fn set_snapshot(&self, snapshot: DidLedgerSnapshot) {
        *self.snapshot.lock().unwrap() = snapshot;
    }
}

#[async_trait]
impl Backend for RecordingBackend {
    async fn submit_tx(&self, tx: BuiltTx) -> Result<FinalizedTxData, BackendError> {
        let call = DidContractCall::decode(&tx.bytes)?;
        let mut calls = self.calls.lock().unwrap();
        calls.push(call);
        // Synthesise deterministic finalisation data from the recorded
        // count + the envelope bytes so callers that look at
        // `tx_hash` / `block_height` see consistent values.
        let block_height = calls.len() as u64;
        let tx_hash = synth_tx_hash(&tx.bytes);
        Ok(FinalizedTxData { tx_hash, block_height })
    }

    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError> {
        Ok(self.state.lock().unwrap().clone())
    }

    async fn read_snapshot(&self) -> Result<DidLedgerSnapshot, BackendError> {
        self.calls.lock().unwrap().push(DidContractCall::ReadLedger);
        Ok(self.snapshot.lock().unwrap().clone())
    }
}

/// Deterministic synthetic tx hash for the recording backend. Uses a
/// short hex prefix of blake2b-256 — opaque to consumers, deterministic
/// for tests.
fn synth_tx_hash(bytes: &[u8]) -> String {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    // 16 hex chars (8 bytes) is plenty for the tests + keeps the
    // assertion-friendly short form.
    hex::encode(&out[..8])
}

// ─────────────────────────────────────────────────────────────────────
// ResolverBackend
// ─────────────────────────────────────────────────────────────────────

/// Read-only backend for the resolver consumer.
///
/// [`Backend::submit_tx`] always returns [`BackendError::ReadOnly`].
/// [`Backend::read_state`] returns a clone of the [`ChargedState`] supplied
/// at construction; [`Backend::read_snapshot`] returns a clone of the
/// [`DidLedgerSnapshot`] supplied at construction. Drops the wallet / proof-server /
/// indexer dep cone for consumers that only need the resolve path.
pub struct ResolverBackend {
    /// [`ChargedState`] served on every [`Backend::read_state`] call.
    pub state: ChargedState<DefaultDB>,
    /// Snapshot served on every [`Backend::read_snapshot`] call.
    pub snapshot: DidLedgerSnapshot,
}

impl fmt::Debug for ResolverBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolverBackend")
            .field("state", &"<ChargedState<DefaultDB>>")
            .field("snapshot", &"<DidLedgerSnapshot>")
            .finish()
    }
}

impl ResolverBackend {
    /// Construct a [`ResolverBackend`] over `state` + an empty snapshot.
    pub fn new(state: ChargedState<DefaultDB>) -> Self {
        Self {
            state,
            snapshot: DidLedgerSnapshot::default(),
        }
    }

    /// Construct a [`ResolverBackend`] over a specific snapshot (empty
    /// raw state).
    pub fn with_snapshot(snapshot: DidLedgerSnapshot) -> Self {
        Self {
            state: empty_charged_state::<DefaultDB>(),
            snapshot,
        }
    }
}

#[async_trait]
impl Backend for ResolverBackend {
    async fn submit_tx(&self, _tx: BuiltTx) -> Result<FinalizedTxData, BackendError> {
        Err(BackendError::ReadOnly)
    }

    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError> {
        Ok(self.state.clone())
    }

    async fn read_snapshot(&self) -> Result<DidLedgerSnapshot, BackendError> {
        Ok(self.snapshot.clone())
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_call::DidContractCall;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn recording_backend_decodes_and_records_submit() {
        let rt = rt();
        let backend = RecordingBackend::new();
        let call1 = DidContractCall::Deactivate;
        let call2 = DidContractCall::RotateControllerKey {
            new_public_key: [3u8; 32],
        };
        let tx1 = BuiltTx { bytes: call1.encode() };
        let tx2 = BuiltTx { bytes: call2.encode() };
        let f1 = rt.block_on(backend.submit_tx(tx1)).unwrap();
        let f2 = rt.block_on(backend.submit_tx(tx2)).unwrap();
        assert_eq!(f1.block_height, 1);
        assert_eq!(f2.block_height, 2);
        assert!(!f1.tx_hash.is_empty());
        let recorded = backend.recorded_calls();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], call1);
        assert_eq!(recorded[1], call2);
    }

    #[test]
    fn recording_backend_submit_rejects_garbage_envelope() {
        let rt = rt();
        let backend = RecordingBackend::new();
        let res = rt.block_on(backend.submit_tx(BuiltTx { bytes: vec![0xff, 0xfe] }));
        assert!(matches!(res, Err(BackendError::Decode(_))));
        // No call recorded on decode failure.
        assert_eq!(backend.recorded_calls().len(), 0);
    }

    #[test]
    fn recording_backend_read_snapshot_records_synthetic_read_ledger() {
        let rt = rt();
        let mut snap = DidLedgerSnapshot::default();
        snap.version = 7;
        let backend = RecordingBackend::with_snapshot(snap.clone());
        let read = rt.block_on(backend.read_snapshot()).unwrap();
        assert_eq!(read, snap);
        assert_eq!(backend.recorded_calls(), vec![DidContractCall::ReadLedger]);
    }

    #[test]
    fn resolver_backend_rejects_submit() {
        let rt = rt();
        let backend = ResolverBackend::new(empty_charged_state::<DefaultDB>());
        let res = rt.block_on(backend.submit_tx(BuiltTx::default()));
        assert_eq!(res, Err(BackendError::ReadOnly));
    }

    #[test]
    fn resolver_backend_returns_state_snapshot() {
        let rt = rt();
        let sentinel = empty_charged_state::<DefaultDB>();
        let backend = ResolverBackend::new(sentinel.clone());
        let read = rt.block_on(backend.read_state()).expect("read_state");
        assert_eq!(read, sentinel);
    }

    #[test]
    fn resolver_backend_returns_snapshot() {
        let rt = rt();
        let mut snap = DidLedgerSnapshot::default();
        snap.version = 42;
        let backend = ResolverBackend::with_snapshot(snap.clone());
        let got = rt.block_on(backend.read_snapshot()).unwrap();
        assert_eq!(got, snap);
    }
}
