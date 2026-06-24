//! I/O substrate abstraction for the Midnight DID contract.
//!
//! The [`Backend`] trait is the two-method seam between
//! [`crate::contract`]'s circuit-call surface and whichever stack is
//! actually shuttling bytes to a Midnight node. Production code wraps a
//! wallet SDK + proof server + indexer in [`LiveBackend`]; api-layer
//! tests use [`RecordingBackend`] (in-memory, records every submit);
//! the resolver consumer uses [`ResolverBackend`] (read-only snapshot).
//!
//! See `doc/specs/2026-06-24-r2-contract-abstraction-design.md` for the
//! full R2 design and the rationale for collapsing `DidContract` into a
//! concrete `Contract<B>` parameterised on `B: Backend`. This module
//! lands the trait + three impls in R2-1; the api-layer migration is
//! R2-2; deletion of the legacy `DidContract` trait is R2-3.

use std::fmt;
use std::sync::Mutex;

use async_trait::async_trait;
use compact_runtime::{empty_charged_state, ChargedState, DefaultDB};

/// A transaction built and proven by the upstream wallet + proof stack,
/// ready for submission via [`Backend::submit_tx`].
///
/// R2-1 introduces this as an opaque placeholder so the [`Backend`]
/// trait surface can land before the wallet SDK port is in place. R2's
/// follow-up will replace this with the actual ledger-level transaction
/// type (likely a re-export of the upstream Midnight transaction shape).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuiltTx {
    /// Opaque proven-transaction bytes. The exact wire shape is owned
    /// by the upstream ledger / proof stack and will be re-typed in the
    /// R2 follow-up that wires [`LiveBackend`].
    pub bytes: Vec<u8>,
}

/// Finalisation data for a submitted transaction.
///
/// Mirrors the shape of `midnight_did_api::contract::FinalizedTxData`
/// in R2-1 to keep the api crate's surface stable across the
/// migration. R2-2 will collapse the duplicate definition once the api
/// crate depends on this crate directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinalizedTxData {
    /// Transaction hash (hex). Empty in mock implementations.
    pub tx_hash: String,
    /// Block height the transaction was included in. Zero in mocks.
    pub block_height: u64,
}

/// Errors raised by a [`Backend`] implementation.
///
/// Narrow, I/O-focused. High-level circuit-call failures continue to
/// surface as `ContractError` in the api crate (unchanged by R2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// Network / RPC failure talking to the Midnight node or indexer.
    Network(String),
    /// The on-chain state could not be decoded into a
    /// [`ChargedState<DefaultDB>`].
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
/// Two methods. Submit a (proven, signed) transaction; read the
/// current contract state. Everything above this — the 12 circuit
/// methods, the snapshot mapper, the operation builders — is identical
/// concrete code regardless of which `Backend` impl is plugged in.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Submit a built transaction (already proven + signed by the
    /// upstream stack) and return its finalisation data.
    async fn submit_tx(&self, tx: BuiltTx) -> Result<FinalizedTxData, BackendError>;

    /// Read the current contract state from the indexer / public-data
    /// provider, returning the raw [`ChargedState<DefaultDB>`] that
    /// `Ledger::<DefaultDB>::new()` can decode.
    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError>;
}

// ─────────────────────────────────────────────────────────────────────
// LiveBackend
// ─────────────────────────────────────────────────────────────────────

/// Production backend: wallet SDK + proof server + indexer.
///
/// # TODO
///
/// R2-1 ships this as a placeholder whose methods panic via
/// [`todo!`]. The R2 follow-up wires the real upstream stack:
///
/// - `wallet_sdk` — drives transaction construction + signing.
/// - `proof_server` — produces the halo2 proofs `submit_tx` carries.
/// - `indexer` — services `read_state` via the public-data provider.
///
/// Tracked in ADR 0008 (R2 contract abstraction reform) and the R2
/// design spec (`doc/specs/2026-06-24-r2-contract-abstraction-design.md`).
#[derive(Debug, Default)]
pub struct LiveBackend {
    /// Wallet SDK handle. `()` until R2 follow-up types it.
    pub wallet_sdk: (),
    /// Proof-server client handle. `()` until R2 follow-up types it.
    pub proof_server: (),
    /// Indexer / public-data-provider client. `()` until R2 follow-up.
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
}

// ─────────────────────────────────────────────────────────────────────
// RecordingBackend
// ─────────────────────────────────────────────────────────────────────

/// In-memory mock backend used by api-layer tests.
///
/// Records every [`Backend::submit_tx`] call as a [`BuiltTx`] in order.
/// [`Backend::read_state`] returns a clone of a settable snapshot
/// (defaults to [`empty_charged_state`]).
///
/// Replaces the R1-era `RecordingContract` mock once R2-2 migrates the
/// operation builders to depend on `Contract<B>` instead of
/// `&dyn DidContract`.
pub struct RecordingBackend {
    txs: Mutex<Vec<BuiltTx>>,
    state: Mutex<ChargedState<DefaultDB>>,
}

impl fmt::Debug for RecordingBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let txs_len = self.txs.lock().map(|v| v.len()).unwrap_or(0);
        f.debug_struct("RecordingBackend")
            .field("recorded_tx_count", &txs_len)
            .field("state", &"<ChargedState<DefaultDB>>")
            .finish()
    }
}

impl Default for RecordingBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingBackend {
    /// Construct a fresh [`RecordingBackend`] with no recorded txs and
    /// an empty [`ChargedState`].
    pub fn new() -> Self {
        Self {
            txs: Mutex::new(Vec::new()),
            state: Mutex::new(empty_charged_state::<DefaultDB>()),
        }
    }

    /// Construct a [`RecordingBackend`] seeded with a specific state.
    pub fn with_state(state: ChargedState<DefaultDB>) -> Self {
        Self {
            txs: Mutex::new(Vec::new()),
            state: Mutex::new(state),
        }
    }

    /// Snapshot of every [`BuiltTx`] that has been submitted, in order.
    pub fn recorded_txs(&self) -> Vec<BuiltTx> {
        self.txs.lock().unwrap().clone()
    }

    /// Replace the state returned by [`Backend::read_state`].
    pub fn set_state(&self, state: ChargedState<DefaultDB>) {
        *self.state.lock().unwrap() = state;
    }
}

#[async_trait]
impl Backend for RecordingBackend {
    async fn submit_tx(&self, tx: BuiltTx) -> Result<FinalizedTxData, BackendError> {
        self.txs.lock().unwrap().push(tx);
        // R2-1: return a stub. R2-2 will decide whether tests want a
        // deterministic synthetic hash / block height or an opt-in seam
        // for asserting on those fields.
        Err(BackendError::Other(
            "RecordingBackend: stub submit_tx".into(),
        ))
    }

    async fn read_state(&self) -> Result<ChargedState<DefaultDB>, BackendError> {
        Ok(self.state.lock().unwrap().clone())
    }
}

// ─────────────────────────────────────────────────────────────────────
// ResolverBackend
// ─────────────────────────────────────────────────────────────────────

/// Read-only backend for the resolver consumer.
///
/// [`Backend::submit_tx`] always returns [`BackendError::ReadOnly`].
/// [`Backend::read_state`] returns a clone of the snapshot supplied at
/// construction. Drops the wallet / proof-server / indexer dep cone for
/// consumers that only need the resolve path.
pub struct ResolverBackend {
    /// Snapshot served on every [`Backend::read_state`] call.
    pub state: ChargedState<DefaultDB>,
}

impl fmt::Debug for ResolverBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolverBackend")
            .field("state", &"<ChargedState<DefaultDB>>")
            .finish()
    }
}

impl ResolverBackend {
    /// Construct a [`ResolverBackend`] over `state`.
    pub fn new(state: ChargedState<DefaultDB>) -> Self {
        Self { state }
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
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn recording_backend_records_submit() {
        let rt = rt();
        let backend = RecordingBackend::new();
        let tx1 = BuiltTx {
            bytes: vec![0xAA, 0xBB],
        };
        let tx2 = BuiltTx {
            bytes: vec![0xCC, 0xDD],
        };
        // Both submits are expected to return the stub error; what we
        // assert on is the recorded sequence, not the result.
        let _ = rt.block_on(backend.submit_tx(tx1.clone()));
        let _ = rt.block_on(backend.submit_tx(tx2.clone()));
        let recorded = backend.recorded_txs();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], tx1);
        assert_eq!(recorded[1], tx2);
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
}
