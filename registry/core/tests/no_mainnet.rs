//! Enforcement of ARCHITECTURE.md §2: mainnet is unrepresentable.
//!
//! §2 is a claim about the binary, not a coding convention. This file is what
//! makes it checkable. If any mainnet constant reaches the registry — by a
//! typo, a copy-paste from a mainnet-capable library, or someone deciding the
//! rule was inconvenient — these tests fail and the build stops.
//!
//! Every accepted risk in §8 depends on this holding. Do not weaken a test
//! here to make a change pass.

use thadam_registry::{BitcoinNetwork, EnvironmentId, ENVIRONMENTS};

/// Bitcoin mainnet's encoding constants, recorded here *only* so that their
/// absence can be asserted. Sources: Bitcoin Core
/// `src/kernel/chainparams.cpp` (`CMainParams`), BIP-32, SLIP-0044.
mod mainnet {
    pub const BECH32_HRP: &str = "bc";
    pub const BASE58_P2PKH_VERSION: u8 = 0x00;
    pub const BASE58_P2SH_VERSION: u8 = 0x05;
    pub const BASE58_WIF_VERSION: u8 = 0x80;
    pub const BIP32_PUBLIC_VERSION: u32 = 0x0488_B21E;
    pub const BIP32_PRIVATE_VERSION: u32 = 0x0488_ADE4;
    pub const BIP44_COIN_TYPE: u32 = 0;
}

#[test]
fn no_bitcoin_network_carries_a_mainnet_constant() {
    for network in BitcoinNetwork::ALL {
        let p = network.params();

        assert_ne!(
            p.bech32_hrp, mainnet::BECH32_HRP,
            "{network} has the mainnet bech32 HRP"
        );
        assert_ne!(
            p.base58_p2pkh_version,
            mainnet::BASE58_P2PKH_VERSION,
            "{network} has the mainnet P2PKH version byte"
        );
        assert_ne!(
            p.base58_p2sh_version,
            mainnet::BASE58_P2SH_VERSION,
            "{network} has the mainnet P2SH version byte"
        );
        assert_ne!(
            p.base58_wif_version,
            mainnet::BASE58_WIF_VERSION,
            "{network} has the mainnet WIF version byte"
        );
        assert_ne!(
            p.bip32_public_version,
            mainnet::BIP32_PUBLIC_VERSION,
            "{network} has the mainnet xpub version bytes"
        );
        assert_ne!(
            p.bip32_private_version,
            mainnet::BIP32_PRIVATE_VERSION,
            "{network} has the mainnet xprv version bytes"
        );
        assert_ne!(
            p.bip44_coin_type,
            mainnet::BIP44_COIN_TYPE,
            "{network} has the mainnet SLIP-0044 coin type"
        );
    }
}

#[test]
fn mainnet_does_not_parse_as_an_environment_id() {
    // Not "is rejected by validation" — there is no value of EnvironmentId
    // that names mainnet, so there is nothing for a parser to return.
    for input in [
        "bitcoin:mainnet",
        "bitcoin:main",
        "bitcoin:bitcoin",
        "bitcoin:livenet",
        "BITCOIN:MAINNET",
    ] {
        assert!(
            input.parse::<EnvironmentId>().is_err(),
            "{input} parsed into an environment id"
        );
    }
}

#[test]
fn no_environment_in_the_catalogue_is_named_like_mainnet() {
    for environment in ENVIRONMENTS {
        let name = environment.id.environment_name();
        assert!(
            !matches!(name, "mainnet" | "main" | "livenet"),
            "catalogue contains an environment named {name}"
        );
    }
}

/// Executable documentation of the hazard in ARCHITECTURE.md §8.
///
/// testnet4 and signet share an address encoding. If this test ever fails, one
/// of them has been given distinguishing parameters, and the warnings written
/// around that collision need revisiting — including whether the UI still
/// needs to display the network next to every address.
#[test]
fn testnet4_and_signet_are_indistinguishable_by_address() {
    let testnet4 = BitcoinNetwork::Testnet4.params();
    let signet = BitcoinNetwork::Signet.params();

    assert_eq!(
        testnet4, signet,
        "testnet4 and signet no longer share an encoding; revisit §8"
    );
}

/// Regtest is the one test network whose address prefix identifies it.
#[test]
fn regtest_is_distinguishable_by_address() {
    assert_ne!(
        BitcoinNetwork::Regtest.params().bech32_hrp,
        BitcoinNetwork::Testnet4.params().bech32_hrp
    );
}
