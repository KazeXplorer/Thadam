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

// ─── Wallet creation ─────────────────────────────────────────────────────────

/// Generates a new BIP-39 mnemonic phrase.
///
/// `bits` must be 128 (12 words), 192 (18 words), or 256 (24 words).
/// The phrase comes from OS CSPRNG entropy; it is never stored here.
///
/// The caller (Kotlin) must encrypt and store it via `EncryptedSharedPreferences`
/// immediately and then pass it back into signing functions on demand.
#[uniffi::export]
pub fn create_mnemonic(bits: u32) -> Result<String, FfiError> {
    let entropy = generate_entropy(bits)?;
    let mnemonic = mnemonic_from_entropy(&entropy)?;
    Ok(mnemonic.to_string())
}

/// Validates a BIP-39 recovery phrase without deriving any keys.
///
/// Returns `Ok(())` when the phrase is valid. Returns an error if any word
/// is not in the BIP-39 word list, the checksum fails, or the word count
/// is not 12, 18, or 24.
#[uniffi::export]
pub fn validate_mnemonic(phrase: String) -> Result<(), FfiError> {
    parse_mnemonic(&phrase)?;
    Ok(())
}

// ─── Address derivation ──────────────────────────────────────────────────────

/// Derives one BIP-84 P2WPKH receive address.
///
/// `account`, `change` (0 = receive, 1 = change), and `index` map directly
/// onto the BIP-84 path `m/84'/coin_type'/account'/change/index`.
///
/// The phrase is borrowed and not stored. The returned string is the bech32
/// address (starts with `tb1` on Testnet4/Signet, `bcrt1` on Regtest).
#[uniffi::export]
pub fn derive_address(
    phrase: String,
    passphrase: String,
    network: NetworkKind,
    account: u32,
    change: u32,
    index: u32,
) -> Result<String, FfiError> {
    let mnemonic = parse_mnemonic(&phrase)?;
    let address = receive_address(&mnemonic, &passphrase, to_registry_network(network), account, change, index)?;
    Ok(address.to_string())
}

// ─── Build ───────────────────────────────────────────────────────────────────

/// Selects coins and assembles an unsigned P2WPKH transaction.
///
/// # Fee
///
/// `fee_rate_sat_vb` is in satoshis per virtual byte. The builder converts it
/// internally. The actual fee may exceed the requested rate when dust change is
/// absorbed (the review's `effective_fee_rate_sat_vb` reflects this).
///
/// # Change
///
/// Change goes to `change_address`. If the leftover after fee would be below
/// dust, it is absorbed into the fee and `has_change` is `false`.
///
/// # Errors
///
/// Fails if the UTXOs are insufficient to cover `amount_sat` plus fees, if any
/// address is malformed, or if the networks are mixed.
#[uniffi::export]
pub fn build_transaction(
    utxos: Vec<FfiUtxo>,
    recipient: String,
    amount_sat: u64,
    fee_rate_sat_vb: u64,
    change_address: String,
    network: NetworkKind,
) -> Result<BuildResult, FfiError> {
    let net = to_registry_network(network);

    let recipient_addr = Address::parse(&recipient, net)
        .map_err(|e| FfiError::Parse { message: e.to_string() })?;
    let change_addr = Address::parse(&change_address, net)
        .map_err(|e| FfiError::Parse { message: e.to_string() })?;

    let fee_rate = FeeRate::from_sat_per_vb(fee_rate_sat_vb)
        .ok_or_else(|| FfiError::Parse {
            message: format!("fee_rate_sat_vb={fee_rate_sat_vb} is zero or overflows"),
        })?;

    // Convert FfiUtxo → builder::Utxo. Keep the FfiUtxo alongside for path lookup.
    let utxo_pairs: Vec<(Utxo, &FfiUtxo)> = utxos
        .iter()
        .map(|fu| {
            let txid = fu.txid.parse::<Txid>().map_err(|_| FfiError::Parse {
                message: format!("invalid txid: {}", fu.txid),
            })?;
            let outpoint = OutPoint { txid, vout: fu.vout };
            let address = Address::parse(&fu.address, net)
                .map_err(|e| FfiError::Parse { message: e.to_string() })?;
            Ok((
                Utxo {
                    outpoint,
                    value: Amount::from_sat(fu.value_sat),
                    address,
                },
                fu,
            ))
        })
        .collect::<Result<_, FfiError>>()?;

    let builder_utxos: Vec<Utxo> = utxo_pairs.iter().map(|(u, _)| u.clone()).collect();

    let intent = Intent {
        recipient: recipient_addr,
        amount: Amount::from_sat(amount_sat),
        change_address: change_addr,
        fee_rate,
    };

    let payload = thadam_builder::build(&builder_utxos, &intent)?;

    // Map each selected UTXO back to the FfiUtxo that has the BIP-84 path.
    // The match is by outpoint — txid + vout uniquely identifies a UTXO.
    let signing_inputs: Vec<FfiSigningInput> = payload
        .selected
        .iter()
        .map(|selected_utxo| {
            let ffi_utxo = utxo_pairs
                .iter()
                .find(|(u, _)| u.outpoint == selected_utxo.outpoint)
                .map(|(_, fu)| *fu)
                .ok_or_else(|| FfiError::Build {
                    message: "selected UTXO not found in input list".to_owned(),
                })?;
            Ok(FfiSigningInput {
                value_sat: selected_utxo.value.to_sat(),
                account: ffi_utxo.account,
                change: ffi_utxo.change,
                index: ffi_utxo.index,
            })
        })
        .collect::<Result<_, FfiError>>()?;

    let unsigned_tx_hex = bitcoin_encode::serialize_hex(&payload.transaction);

    let change_amount_sat = payload
        .review
        .outputs
        .iter()
        .find(|o| o.is_change)
        .map(|o| o.amount.to_sat());

    // effective_fee_rate is in sat/kwu; convert to sat/vB for the UI.
    // sat/vB = sat/kwu * 4 / 1000 (1 vByte = 4 weight units; 1 kwu = 1000 wu)
    let effective_fee_rate_sat_vb =
        payload.review.effective_fee_rate.to_sat_per_kwu() as f64 * 4.0 / 1000.0;

    Ok(BuildResult {
        unsigned_tx_hex,
        signing_inputs,
        fee_sat: payload.review.fee.to_sat(),
        effective_fee_rate_sat_vb,
        recipient_amount_sat: payload.review.amount_leaving_the_wallet().to_sat(),
        change_amount_sat,
        has_change: payload.review.has_change(),
        vsize: payload.review.vsize,
        input_count: payload.review.input_count as u64,
    })
}

// ─── Sign ────────────────────────────────────────────────────────────────────

/// Signs a previously built unsigned transaction.
///
/// # Inputs
///
/// - `unsigned_tx_hex`: the `unsigned_tx_hex` field from [`BuildResult`], unchanged.
/// - `signing_inputs`: the `signing_inputs` field from [`BuildResult`], unchanged.
/// - `phrase` / `passphrase`: the wallet's recovery phrase and optional passphrase.
///   These are borrowed and not stored by this function.
/// - `network`: must match the network used in `build_transaction`.
///
/// # Returns
///
/// The signed transaction, consensus-serialised and hex-encoded. This hex
/// string is what is broadcast to the Bitcoin network.
///
/// # Errors
///
/// Fails if the transaction hex cannot be parsed, if the input count mismatches,
/// or if any key derivation step fails.
#[uniffi::export]
pub fn sign_built_transaction(
    unsigned_tx_hex: String,
    signing_inputs: Vec<FfiSigningInput>,
    phrase: String,
    passphrase: String,
    network: NetworkKind,
) -> Result<String, FfiError> {
    let tx: bitcoin::Transaction = bitcoin_encode::deserialize_hex(&unsigned_tx_hex)
        .map_err(|e| FfiError::Parse {
            message: format!("cannot parse transaction hex: {e}"),
        })?;

    let mnemonic = parse_mnemonic(&phrase).map_err(FfiError::from)?;

    let inputs: Vec<SigningInput> = signing_inputs
        .iter()
        .map(|si| SigningInput {
            value: Amount::from_sat(si.value_sat),
            account: si.account,
            change: si.change,
            index: si.index,
        })
        .collect();

    let signed = sign_transaction(tx, &inputs, &mnemonic, &passphrase, to_registry_network(network))
        .map_err(|e| FfiError::Sign { message: e.to_string() })?;

    Ok(bitcoin_encode::serialize_hex(&signed))
}
