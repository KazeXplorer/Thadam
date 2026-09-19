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
//! Signing. There is nothing to sign until a transaction builder exists, and
//! §10 says abstractions are extracted, not anticipated — so the signer *port*
//! is not defined either.
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
use bitcoin::CompressedPublicKey;
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
