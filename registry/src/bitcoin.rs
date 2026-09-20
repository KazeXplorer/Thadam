//! Bitcoin networks and their consensus-adjacent constants.

use core::fmt;
use core::str::FromStr;

/// The Bitcoin networks Thadam can reach.
///
/// # There is deliberately no `Default`
///
/// README §2: a user can never be on a network they did not choose. A default
/// is a network chosen by the code, so this type does not have one and must
/// not grow one. Every function that needs a network takes it as an argument.
///
/// testnet3 is absent on purpose: its difficulty reset rule makes it unusable
/// for timing-sensitive work, and its address encoding is indistinguishable
/// from testnet4's and signet's (README §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BitcoinNetwork {
    /// The Bitcoin network carrying real money.
    ///
    /// Every mistake here is permanent and expensive. Nothing in this codebase
    /// may select this variant on a user's behalf.
    Mainnet,
    /// The testnet introduced in Bitcoin Core 28.0.
    Testnet4,
    /// The default global signet. Thadam's staging chain.
    Signet,
    /// A locally run chain with no shared history. Thadam's integration-test
    /// chain.
    Regtest,
}

impl BitcoinNetwork {
    /// Every variant of this enum.
    pub const ALL: &'static [BitcoinNetwork] = &[
        BitcoinNetwork::Mainnet,
        BitcoinNetwork::Testnet4,
        BitcoinNetwork::Signet,
        BitcoinNetwork::Regtest,
    ];

    /// The networks that carry no real value.
    ///
    /// Useful for tests that want to exercise every network where being wrong
    /// is free. It is *not* a safety mechanism — nothing may branch on this to
    /// decide whether a user action is allowed.
    pub const TEST_NETWORKS: &'static [BitcoinNetwork] = &[
        BitcoinNetwork::Testnet4,
        BitcoinNetwork::Signet,
        BitcoinNetwork::Regtest,
    ];

    /// The network name used in a `chain:network` identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            BitcoinNetwork::Mainnet => "mainnet",
            BitcoinNetwork::Testnet4 => "testnet4",
            BitcoinNetwork::Signet => "signet",
            BitcoinNetwork::Regtest => "regtest",
        }
    }

    /// Whether this network carries real money.
    ///
    /// For display and for the confirmations README §2 requires — never for
    /// deciding what the code does on the user's behalf.
    pub const fn carries_real_value(self) -> bool {
        match self {
            BitcoinNetwork::Mainnet => true,
            BitcoinNetwork::Testnet4 | BitcoinNetwork::Signet | BitcoinNetwork::Regtest => false,
        }
    }

    /// A name to show a user, in words.
    ///
    /// README §2 requires every screen showing money to name its network in
    /// words rather than relying on colour.
    pub const fn display_name(self) -> &'static str {
        match self {
            BitcoinNetwork::Mainnet => "MAINNET",
            BitcoinNetwork::Testnet4 => "Testnet4",
            BitcoinNetwork::Signet => "Signet",
            BitcoinNetwork::Regtest => "Regtest",
        }
    }

    /// This network's address and key-encoding parameters.
    ///
    /// The exhaustive match means a new variant cannot be added without
    /// supplying its constants here.
    ///
    /// Sources: Bitcoin Core `src/kernel/chainparams.cpp` (`bech32_hrp`,
    /// `base58Prefixes`), BIP-32 (extended key version bytes), SLIP-0044
    /// (coin type).
    pub const fn params(self) -> BitcoinParams {
        match self {
            BitcoinNetwork::Mainnet => BitcoinParams {
                bech32_hrp: "bc",
                base58_p2pkh_version: 0x00,
                base58_p2sh_version: 0x05,
                base58_wif_version: 0x80,
                bip32_public_version: 0x0488_B21E,
                bip32_private_version: 0x0488_ADE4,
                bip44_coin_type: 0,
            },
            // testnet4 and signet are byte-for-byte identical here. That is not
            // an oversight; it is the hazard README §9 names, and
            // `tests/network_safety.rs` asserts it stays true so it cannot be
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

    /// Parses a network name, exactly.
    ///
    /// Case-sensitive and with no aliases. README §2: nothing external may
    /// change the network, so a parser that is generous about input is a
    /// parser that lets a malformed link pick a chain. `"MAINNET"`, `"main"`
    /// and `"Bitcoin"` do not parse.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" => Ok(BitcoinNetwork::Mainnet),
            "testnet4" => Ok(BitcoinNetwork::Testnet4),
            "signet" => Ok(BitcoinNetwork::Signet),
            "regtest" => Ok(BitcoinNetwork::Regtest),
            _ => Err(()),
        }
    }
}

/// Address and key-encoding parameters for one Bitcoin network.
///
/// These are constants of the network, not configuration: they have never
/// changed for any network here and will not.
///
/// This struct holds no logic. Encoding an address is the signer's job; the
/// registry only says which constants that encoding must use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitcoinParams {
    /// Human-readable part of a bech32 (BIP-173) or bech32m (BIP-350) address.
    ///
    /// # This does not identify a network
    ///
    /// testnet3, testnet4 and signet all use `tb`. An address alone cannot tell
    /// you which of them it belongs to, and sending to the wrong one succeeds
    /// silently. Mainnet (`bc`) and regtest (`bcrt`) *are* distinguishable.
    /// README §9.
    pub bech32_hrp: &'static str,
    /// Version byte prefixing a base58check P2PKH address.
    pub base58_p2pkh_version: u8,
    /// Version byte prefixing a base58check P2SH address.
    pub base58_p2sh_version: u8,
    /// Version byte prefixing a base58check WIF private key.
    pub base58_wif_version: u8,
    /// BIP-32 extended *public* key version bytes (`xpub` / `tpub`).
    pub bip32_public_version: u32,
    /// BIP-32 extended *private* key version bytes (`xprv` / `tprv`).
    pub bip32_private_version: u32,
    /// SLIP-0044 coin type, the second level of a BIP-44 derivation path.
    ///
    /// Mainnet is 0; every Bitcoin test network is 1. This is why one recovery
    /// phrase yields two entirely separate wallets with no overlap.
    pub bip44_coin_type: u32,
}
