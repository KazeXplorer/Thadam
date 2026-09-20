//! Thadam signer — recovery phrase, key derivation, address encoding.
//!
//! ARCHITECTURE.md §3: this is the only layer with keys. Nothing above it sees
//! key material, and nothing here knows about transactions, the UI, or the
//! chain service.
//!
//! # What this crate does and does not do
//!
//! It does **not** implement any BIP. bech32, BIP-32, BIP-39 and base58check
//! come from `rust-bitcoin` and `bip39`; the elliptic curve arithmetic comes
//! from `libsecp256k1` by way of the `secp256k1` crate.
//!
//! What this crate owns is the *wiring*, and that is where its bugs would live:
//!
//! - which network's constants a derivation uses ([`bip84_path`])
//! - that an address is never separated from the network it belongs to
//!   ([`Address`])
//! - that no code path can reach mainnet ([`network`])
//!
//! # Why it stopped hand-rolling
//!
//! An earlier version wrote all of the above by hand, deliberately, because
//! §1 values understanding. It shipped a real bug: BIP-39 requires NFKD
//! normalisation of the mnemonic and passphrase, the hand-written version
//! omitted it, and a passphrase containing a combining accent silently derived
//! a different wallet from every other implementation — no error, no way for a
//! user to notice. It was found only by testing against the crates that now do
//! the work.
//!
//! With real users, unreviewed cryptography is a liability that no amount of
//! learning value justifies. §9 records the decision.
//!
//! # What is not here yet
//!
//! Encrypted storage of the phrase (§5) is also absent: this crate derives, it
//! does not persist.
//!
//! # Example
//!
//! ```
//! use thadam_registry::BitcoinNetwork;
//! use thadam_signer::{parse_mnemonic, receive_address};
//!
//! let mnemonic = parse_mnemonic(
//!     "abandon abandon abandon abandon abandon abandon \
//!      abandon abandon abandon abandon abandon about",
//! )?;
//! let address = receive_address(&mnemonic, "", BitcoinNetwork::Testnet4, 0, 0, 0)?;
//! assert!(address.to_string().starts_with("tb1"));
//! assert_eq!(address.network(), BitcoinNetwork::Testnet4);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod address;
pub mod network;

use core::fmt;

use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
use bitcoin::ecdsa;
use bitcoin::hashes::{hash160, Hash};
use bitcoin::sighash::{EcdsaSighashType, SighashCache};
use bitcoin::CompressedPublicKey;
use rand::rngs::OsRng;
use rand::RngCore;
use secp256k1::Secp256k1;
use thadam_registry::BitcoinNetwork;

pub use address::Address;
pub use bip39::Mnemonic;

use crate::network::to_library_network;

/// BIP-84 purpose: native segwit (P2WPKH) accounts.
pub const BIP84_PURPOSE: u32 = 84;

/// Anything that can go wrong between a phrase and an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The recovery phrase was malformed — wrong word count, an unknown word,
    /// or a checksum that does not match.
    InvalidMnemonic(String),
    /// Key derivation failed.
    Derivation(String),
    /// The string was not a Bitcoin address at all.
    MalformedAddress,
    /// The address is well-formed but belongs to a different network.
    ///
    /// Carries both so a UI can say "that is a regtest address and you are on
    /// testnet4" rather than "invalid address".
    WrongNetwork {
        /// The network the caller said it was on.
        expected: BitcoinNetwork,
        /// The address as given.
        address: String,
    },
    /// The library produced an address whose prefix disagrees with the
    /// registry's constant for that network.
    ///
    /// This should be unreachable. It exists because the alternative to
    /// checking is trusting two sources of truth to agree silently.
    HrpMismatch {
        /// What the registry says the prefix should be.
        expected: String,
        /// What the library actually produced.
        produced: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidMnemonic(detail) => write!(f, "invalid recovery phrase: {detail}"),
            Error::Derivation(detail) => write!(f, "key derivation failed: {detail}"),
            Error::MalformedAddress => f.write_str("not a valid Bitcoin address"),
            Error::WrongNetwork { expected, address } => {
                write!(f, "address {address} does not belong to {expected}")
            }
            Error::HrpMismatch { expected, produced } => write!(
                f,
                "registry expects prefix '{expected}' but the library produced {produced}"
            ),
        }
    }
}

impl std::error::Error for Error {}

/// Parses and validates a BIP-39 recovery phrase.
///
/// Thin wrapper over the `bip39` crate, so callers do not depend on it
/// directly and its error type does not leak into Thadam's API.
pub fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, Error> {
    Mnemonic::parse_normalized(phrase).map_err(|e| Error::InvalidMnemonic(e.to_string()))
}

/// Builds a mnemonic from raw entropy.
///
/// The caller supplies the entropy, because where randomness comes from is a
/// decision that belongs at the edge of the system rather than buried here —
/// and getting it from the wrong place is how real wallets have failed.
pub fn mnemonic_from_entropy(entropy: &[u8]) -> Result<Mnemonic, Error> {
    Mnemonic::from_entropy(entropy).map_err(|e| Error::InvalidMnemonic(e.to_string()))
}

/// Generates cryptographically secure random entropy using the OS CSPRNG.
///
/// `bits` must be 128, 192, or 256 — the three sizes BIP-39 permits. The
/// result is suitable for passing directly to [`mnemonic_from_entropy`].
///
/// The OS entropy source (getrandom syscall) is the correct default for
/// production. It is not injectable here because testability is handled at the
/// [`mnemonic_from_entropy`] level: tests that need determinism supply their
/// own bytes there. An injectable RNG here would be a seam for misuse.
pub fn generate_entropy(bits: u32) -> Result<Vec<u8>, Error> {
    let byte_count = match bits {
        128 => 16,
        192 => 24,
        256 => 32,
        _ => {
            return Err(Error::InvalidMnemonic(format!(
                "entropy must be 128, 192 or 256 bits; got {bits}"
            )))
        }
    };
    let mut entropy = vec![0u8; byte_count];
    OsRng.fill_bytes(&mut entropy);
    Ok(entropy)
}

/// The BIP-84 path for one address: `m/84'/coin_type'/account'/change/index`.
///
/// `coin_type` is taken from the registry rather than written here, so the
/// derivation path cannot disagree with the network it claims to be for. Every
/// Bitcoin test network uses 1; mainnet's 0 is unreachable because
/// [`BitcoinNetwork`] has no mainnet variant.
///
/// This function is one of the few things in this crate that is genuinely
/// Thadam's own logic rather than a library call, which is why it has tests of
/// its own.
pub fn bip84_path(
    network: BitcoinNetwork,
    account: u32,
    change: u32,
    index: u32,
) -> Result<DerivationPath, Error> {
    let steps = [
        ChildNumber::from_hardened_idx(BIP84_PURPOSE),
        ChildNumber::from_hardened_idx(network.params().bip44_coin_type),
        ChildNumber::from_hardened_idx(account),
        ChildNumber::from_normal_idx(change),
        ChildNumber::from_normal_idx(index),
    ];

    let mut path = Vec::with_capacity(steps.len());
    for step in steps {
        path.push(step.map_err(|e| Error::Derivation(e.to_string()))?);
    }
    Ok(DerivationPath::from(path))
}

/// Derives one receive address from a phrase.
///
/// The whole slice in one function: phrase → seed → master key → BIP-84 path →
/// public key → address, with the network's constants supplied by the registry
/// at both ends.
pub fn receive_address(
    mnemonic: &Mnemonic,
    passphrase: &str,
    network: BitcoinNetwork,
    account: u32,
    change: u32,
    index: u32,
) -> Result<Address, Error> {
    let secp = Secp256k1::new();
    // `to_seed`, not `to_seed_normalized`. The latter means "the passphrase is
    // ALREADY NFKD-normalised" and skips the step — which reintroduces exactly
    // the bug the hand-written version had, now with a library's name on it.
    // `tests/wiring.rs` pins this; it caught the mistake on the first run.
    let seed = mnemonic.to_seed(passphrase);

    let master = Xpriv::new_master(to_library_network(network), &seed)
        .map_err(|e| Error::Derivation(e.to_string()))?;
    let derived = master
        .derive_priv(&secp, &bip84_path(network, account, change, index)?)
        .map_err(|e| Error::Derivation(e.to_string()))?;

    let private_key = derived.to_priv();
    let public_key = CompressedPublicKey::from_private_key(&secp, &private_key)
        .map_err(|e| Error::Derivation(e.to_string()))?;

    Address::p2wpkh(&public_key, network)
}

/// One input's signing context — everything needed to re-derive the key and
/// compute the BIP-143 sighash, with nothing secret included.
///
/// The values here come from the UTXO set (public blockchain data) and the
/// wallet's own key book. The mnemonic is passed separately to `sign_transaction`
/// and is never part of this struct.
#[derive(Debug, Clone)]
pub struct SigningInput {
    /// Value of the UTXO being spent.
    ///
    /// Required for the BIP-143 SegWit v0 sighash computation. An unsigned
    /// transaction does not carry input values, so they must be supplied by
    /// the caller who fetched the UTXO set.
    pub value: bitcoin::Amount,
    /// BIP-84 account index (`m/84'/coin'/account'`).
    pub account: u32,
    /// BIP-84 change flag: 0 = external (receive), 1 = internal (change).
    pub change: u32,
    /// BIP-84 address index.
    pub index: u32,
}

/// Signs every input of a P2WPKH transaction (BIP-143 sighash, ECDSA).
///
/// # How it works
///
/// Each input gets its own key re-derived from the mnemonic via the BIP-84
/// path in the corresponding [`SigningInput`]. The SegWit v0 sighash commits
/// to the input's value, which is why that value must be supplied even though
/// it does not appear in the unsigned transaction bytes.
///
/// After this function returns, the transaction's witnesses are filled and the
/// transaction can be serialised and broadcast.
///
/// # Key handling
///
/// Keys are derived on demand and dropped immediately. The mnemonic is
/// borrowed, not stored. Nothing leaves this call except the signed bytes.
///
/// # Errors
///
/// Returns [`Error::Derivation`] if the `inputs` slice length does not match
/// the transaction's input count, or if any derivation step fails.
pub fn sign_transaction(
    mut unsigned_tx: bitcoin::Transaction,
    inputs: &[SigningInput],
    mnemonic: &Mnemonic,
    passphrase: &str,
    network: BitcoinNetwork,
) -> Result<bitcoin::Transaction, Error> {
    if inputs.len() != unsigned_tx.input.len() {
        return Err(Error::Derivation(format!(
            "got {} signing inputs for {} transaction inputs",
            inputs.len(),
            unsigned_tx.input.len()
        )));
    }

    let secp = Secp256k1::new();
    // `to_seed` — not `to_seed_normalized` — for the same reason as in
    // `receive_address`: the latter skips NFKD normalisation of the passphrase.
    let seed = mnemonic.to_seed(passphrase);
    let master = Xpriv::new_master(to_library_network(network), &seed)
        .map_err(|e| Error::Derivation(e.to_string()))?;

    // Compute all witnesses first (SighashCache holds a shared borrow of the
    // transaction), then apply them (which needs a mutable borrow). This two-
    // phase approach avoids the borrow conflict without unsafe code.
    let witnesses: Vec<bitcoin::Witness> = {
        let mut cache = SighashCache::new(&unsigned_tx);
        inputs
            .iter()
            .enumerate()
            .map(|(i, si)| {
                let path = bip84_path(network, si.account, si.change, si.index)?;
                let derived = master
                    .derive_priv(&secp, &path)
                    .map_err(|e| Error::Derivation(e.to_string()))?;
                let private_key = derived.to_priv();

                // Derive the raw secp256k1 key pair.
                let secret_key = &private_key.inner;
                let raw_pubkey = secp256k1::PublicKey::from_secret_key(&secp, secret_key);
                // Compressed public key bytes: always 33 bytes for a P2WPKH key.
                let pk_bytes = raw_pubkey.serialize();

                // BIP-143 script code for P2WPKH:
                //   OP_DUP OP_HASH160 <hash160(pubkey)> OP_EQUALVERIFY OP_CHECKSIG
                // This is the same as the P2PKH script for the key's hash.
                let key_hash = hash160::Hash::hash(&pk_bytes);
                let wpubkey_hash = bitcoin::WPubkeyHash::from_raw_hash(key_hash);
                let script_pubkey = bitcoin::ScriptBuf::new_p2wpkh(&wpubkey_hash);

                let sighash = cache
                    .p2wpkh_signature_hash(i, &script_pubkey, si.value, EcdsaSighashType::All)
                    .map_err(|e| Error::Derivation(e.to_string()))?;

                let message = secp256k1::Message::from_digest(sighash.to_byte_array());
                let raw_sig = secp.sign_ecdsa(&message, secret_key);
                let ecdsa_sig = ecdsa::Signature {
                    signature: raw_sig,
                    sighash_type: EcdsaSighashType::All,
                };

                // Witness stack for P2WPKH: [<DER sig + sighash byte>, <compressed pubkey>]
                let sig_bytes: Vec<u8> = ecdsa_sig.serialize().to_vec();
                let mut witness = bitcoin::Witness::new();
                witness.push(sig_bytes.as_slice());
                witness.push(pk_bytes.as_slice());
                Ok(witness)
            })
            .collect::<Result<_, Error>>()?
    };

    // Phase 2: apply witnesses.
    for (input, witness) in unsigned_tx.input.iter_mut().zip(witnesses) {
        input.witness = witness;
    }
    Ok(unsigned_tx)
}
