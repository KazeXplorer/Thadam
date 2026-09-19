//! Bitcoin networks and their consensus-adjacent constants.

use core::fmt;
use core::str::FromStr;

/// The Bitcoin networks this build can reach.
///
/// # There is no `Mainnet` variant, and there must never be one
///
/// ARCHITECTURE.md §2. Every accepted risk in §8 — an unaudited
/// implementation, keys in browser memory, best-effort zeroization — is
/// accepted *because* this enum has no mainnet variant. A build that cannot
/// touch real money can be wrong without being expensive.
///
/// Adding a variant here is not a feature. It invalidates the threat model.
///
/// # testnet3 is absent on purpose
///
/// testnet3's difficulty reset rule makes it unusable for timing-sensitive
/// work, and its address encoding is indistinguishable from testnet4's and
/// signet's (see [`BitcoinParams::bech32_hrp`]). Leaving it out removes one
/// way to send coins into a hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BitcoinNetwork {
    /// The testnet introduced in Bitcoin Core 28.0.
    Testnet4,
    /// The default global signet.
    Signet,
    /// A locally run chain with no shared history.
    Regtest,
}

impl BitcoinNetwork {
    /// Every variant of this enum.
    ///
    /// Kept in sync with the enum by `tests/catalogue.rs`, which stops
    /// compiling when a variant is added and then fails on the count.
    pub const ALL: &'static [BitcoinNetwork] = &[
        BitcoinNetwork::Testnet4,
        BitcoinNetwork::Signet,
        BitcoinNetwork::Regtest,
    ];

    /// The environment name used in a `chain:environment` identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            BitcoinNetwork::Testnet4 => "testnet4",
            BitcoinNetwork::Signet => "signet",
            BitcoinNetwork::Regtest => "regtest",
        }
    }

    /// This network's address and key-encoding parameters.
    ///
    /// The exhaustive match means a new variant cannot be added without
    /// supplying its constants here.
    ///
    /// Sources: Bitcoin Core `src/kernel/chainparams.cpp` (`bech32_hrp`,
    /// `base58Prefixes`), BIP-32 (extended key version bytes), SLIP-0044
    /// (coin type: "Testnet (all coins)" = 1).
    pub const fn params(self) -> BitcoinParams {
        match self {
            // testnet4 and signet are byte-for-byte identical here. That is
            // not an oversight; it is the hazard ARCHITECTURE.md §8 names, and
            // `tests/no_mainnet.rs` asserts it stays true so it cannot be
            // forgotten.
            BitcoinNetwork::Testnet4 | BitcoinNetwork::Signet => BitcoinParams {
                bech32_hrp: "tb",
                base58_p2pkh_version: 0x6F,
                base58_p2sh_version: 0xC4,
                base58_wif_version: 0xEF,
                bip32_public_version: 0x0435_87CF,
                bip32_private_version: 0x0435_8394,
                bip44_coin_type: 1,
            },
            // Regtest differs from testnet in exactly one field: the HRP.
            // `bcrt1` is the only Bitcoin test address prefix that identifies
            // its own network.
            BitcoinNetwork::Regtest => BitcoinParams {
                bech32_hrp: "bcrt",
                base58_p2pkh_version: 0x6F,
                base58_p2sh_version: 0xC4,
                base58_wif_version: 0xEF,
                bip32_public_version: 0x0435_87CF,
                bip32_private_version: 0x0435_8394,
                bip44_coin_type: 1,
            },
        }
    }
}

impl fmt::Display for BitcoinNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BitcoinNetwork {
    type Err = ();

    /// Parses an environment name. Unknown names — `mainnet` and `testnet3`
    /// among them — do not parse, and no amount of input makes them.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "testnet4" => Ok(BitcoinNetwork::Testnet4),
            "signet" => Ok(BitcoinNetwork::Signet),
            "regtest" => Ok(BitcoinNetwork::Regtest),
            _ => Err(()),
        }
    }
}

/// Address and key-encoding parameters for one Bitcoin network.
///
/// These are the values a signer needs in order to turn a derived key into an
/// address. They are constants of the network, not configuration: they have
/// never changed for any network here and will not.
///
/// This struct holds no logic. Encoding an address is the signer's job; the
/// registry only says which constants that encoding must use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitcoinParams {
    /// Human-readable part of a bech32 (BIP-173) or bech32m (BIP-350) address.
    ///
    /// # This does not identify a network
    ///
    /// testnet3, testnet4 and signet all use `tb`. An address alone cannot
    /// tell you which chain it belongs to, and sending to the wrong one
    /// succeeds silently. Network is explicit state carried alongside every
    /// address — never inferred from one. ARCHITECTURE.md §8.
    pub bech32_hrp: &'static str,
    /// Version byte prefixing a base58check P2PKH address.
    pub base58_p2pkh_version: u8,
    /// Version byte prefixing a base58check P2SH address.
    pub base58_p2sh_version: u8,
    /// Version byte prefixing a base58check WIF private key.
    pub base58_wif_version: u8,
    /// BIP-32 extended *public* key version bytes (`tpub`).
    pub bip32_public_version: u32,
    /// BIP-32 extended *private* key version bytes (`tprv`).
    pub bip32_private_version: u32,
    /// SLIP-0044 coin type, the second level of a BIP-44 derivation path.
    ///
    /// Every Bitcoin test network uses 1. Mainnet uses 0, which is why 0 is in
    /// the forbidden set in `tests/no_mainnet.rs`.
    pub bip44_coin_type: u32,
}
