//! Thadam FFI boundary.
//!
//! ARCHITECTURE.md §6: this crate is the only place that knows about the
//! mobile platform. Everything above it is pure Rust; nothing above it knows
//! what a JVM or an `.so` is.
//!
//! # Shape
//!
//! Every type exported here is either a primitive, a `Vec`, or a
//! `#[derive(uniffi::Record)]` struct. No `Arc`, no `Mutex`, no mutable
//! shared state — the Kotlin layer manages state (active phrase, selected
//! network, address index). Rust functions here are stateless.
//!
//! # Error handling
//!
//! All errors collapse into [`FfiError`]. Details are in the `message`
//! string, which is human-readable and shown directly in Kotlin's catch block.
//!
//! # Mainnet
//!
//! [`NetworkKind`] has no `Mainnet` variant. Mainnet ships in M2 after the
//! test-net cycle is complete.

// UniFFI generates the JNI scaffolding from proc-macro annotations in this
// file. The namespace defaults to the crate name (`thadam_ffi`), which becomes
// the Kotlin package prefix.
uniffi::setup_scaffolding!();

use bitcoin::{Amount, FeeRate, OutPoint, Txid};
use bitcoin::consensus::encode as bitcoin_encode;
use thadam_builder::{Intent, Utxo};
use thadam_registry::BitcoinNetwork;
use thadam_signer::{
    generate_entropy, mnemonic_from_entropy, parse_mnemonic, receive_address, sign_transaction,
    Address, SigningInput,
};

// ─── Network ─────────────────────────────────────────────────────────────────

/// The Bitcoin networks Thadam supports in this build.
///
/// Mainnet is intentionally absent until the M2 release. Tapping a "Mainnet"
/// chip in the UI does nothing (greyed-out); if Kotlin somehow calls this
/// crate with a mainnet request, there is no variant to map it to.
#[derive(uniffi::Enum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkKind {
    /// Testnet4 — the current primary test network.
    Testnet4,
    /// Signet — useful for scripted scenarios with predictable blocks.
    Signet,
    /// Regtest — local development only.
    Regtest,
}

fn to_registry_network(kind: NetworkKind) -> BitcoinNetwork {
    match kind {
        NetworkKind::Testnet4 => BitcoinNetwork::Testnet4,
        NetworkKind::Signet => BitcoinNetwork::Signet,
        NetworkKind::Regtest => BitcoinNetwork::Regtest,
    }
}

// ─── Error ───────────────────────────────────────────────────────────────────

/// Everything that can go wrong across the FFI boundary.
///
/// Kotlin catches this as a single sealed class; the `message` field is
/// displayable directly in a toast or an error screen.
#[derive(uniffi::Error, Debug, thiserror::Error)]
pub enum FfiError {
    /// The recovery phrase or entropy was invalid.
    #[error("wallet error: {message}")]
    Wallet {
        /// Human-readable detail.
        message: String,
    },
    /// Transaction building failed.
    #[error("build error: {message}")]
    Build {
        /// Human-readable detail.
        message: String,
    },
    /// Transaction signing failed.
    #[error("sign error: {message}")]
    Sign {
        /// Human-readable detail.
        message: String,
    },
    /// An address or transaction could not be parsed.
    #[error("parse error: {message}")]
    Parse {
        /// Human-readable detail.
        message: String,
    },
}

impl From<thadam_signer::Error> for FfiError {
    fn from(e: thadam_signer::Error) -> Self {
        FfiError::Wallet {
            message: e.to_string(),
        }
    }
}

impl From<thadam_builder::Error> for FfiError {
    fn from(e: thadam_builder::Error) -> Self {
        FfiError::Build {
            message: e.to_string(),
        }
    }
}

// ─── UTXO / signing input ────────────────────────────────────────────────────

/// One unspent output the wallet can spend.
///
/// Kotlin populates this from the chain service response. The `account`,
/// `change`, and `index` fields mirror the BIP-84 path used to derive the
/// address that owns this UTXO — the FFI layer uses them to re-derive the
/// signing key.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiUtxo {
    /// Transaction id (64 hex chars, big-endian display order).
    pub txid: String,
    /// Output index within that transaction.
    pub vout: u32,
    /// Value in satoshis.
    pub value_sat: u64,
    /// The bech32 address that owns this output.
    pub address: String,
    /// BIP-84 account index used to derive `address`.
    pub account: u32,
    /// BIP-84 change flag (0 = receive, 1 = change).
    pub change: u32,
    /// BIP-84 address index used to derive `address`.
    pub index: u32,
}

/// Per-input signing context returned by `build_transaction`.
///
/// Kotlin passes this slice, unchanged, back to `sign_transaction`.
/// It carries no secret material — only the UTXO value and the BIP-84
/// path components needed to re-derive the key.
#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiSigningInput {
    /// Value of the UTXO being spent, in satoshis.
    pub value_sat: u64,
    /// BIP-84 account index.
    pub account: u32,
    /// BIP-84 change flag.
    pub change: u32,
    /// BIP-84 address index.
    pub index: u32,
}

// ─── Build result ────────────────────────────────────────────────────────────

/// What `build_transaction` returns to Kotlin.
///
/// The unsigned transaction is serialised as hex so it can be stored,
/// reviewed, and passed back to `sign_transaction` in a later call without
/// keeping any in-memory state. All amounts are in satoshis.
#[derive(uniffi::Record, Debug, Clone)]
pub struct BuildResult {
    /// Unsigned transaction, consensus-serialised and hex-encoded.
    ///
    /// Pass this directly to `sign_transaction`. Do not mutate it — the
    /// signing inputs are computed against this exact byte sequence.
    pub unsigned_tx_hex: String,

    /// One entry per input, in the same order as the inputs in `unsigned_tx_hex`.
    ///
    /// Pass this slice unchanged to `sign_transaction`.
    pub signing_inputs: Vec<FfiSigningInput>,

    // ── Review summary (what the user approves) ──────────────────────────

    /// Fee in satoshis.
    pub fee_sat: u64,
    /// Effective fee rate in sat/vByte (may exceed requested if dust was absorbed).
    pub effective_fee_rate_sat_vb: f64,
    /// Amount going to the recipient, in satoshis.
    pub recipient_amount_sat: u64,
    /// Change amount in satoshis, or `None` when the transaction is changeless.
    pub change_amount_sat: Option<u64>,
    /// Whether the transaction has a change output.
    pub has_change: bool,
    /// Predicted virtual size in vBytes once signed.
    pub vsize: u64,
    /// Number of inputs selected.
    pub input_count: u64,
}
