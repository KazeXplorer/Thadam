//! Tests for the part of this crate that is actually ours.
//!
//! # Why this file replaced the differential suite
//!
//! The previous version of this crate hand-rolled the BIP mechanics and was
//! verified by comparing its output against `rust-bitcoin`. `rust-bitcoin` is
//! now the implementation, so those tests would be comparing a library to
//! itself — green forever, proving nothing. Keeping them would have been worse
//! than deleting them, because a passing suite that cannot fail is a false
//! statement about how well the code is tested.
//!
//! What is left to test is the wiring, which is where this crate's own bugs
//! would live:
//!
//! - does a derivation use the right network's coin type
//! - is an address ever separated from the network it belongs to
//! - does the registry's constant agree with what the library produced
//!
//! The known-answer tests below are meaningful for the first time now. While
//! this crate computed its own addresses, pinning one only asserted that the
//! code still did what it did yesterday. With the cryptography in a library,
//! a pinned address tests *path assembly*: change `BIP84_PURPOSE` to 44, or
//! break the registry's coin type, and these fail immediately.

use thadam_registry::BitcoinNetwork;
use thadam_signer::{bip84_path, mnemonic_from_entropy, parse_mnemonic, receive_address, Address};

/// The all-zeros-entropy phrase. Standard across BIP-39 test material, and
/// worthless as a wallet, which is why it is safe to write down.
const TEST_PHRASE: &str = "abandon abandon abandon abandon abandon abandon \
                           abandon abandon abandon abandon abandon about";

fn test_mnemonic() -> bip39::Mnemonic {
    parse_mnemonic(TEST_PHRASE).expect("the standard test phrase must parse")
}

// --- known answers ----------------------------------------------------------

/// Pinned addresses for the standard test phrase.
///
/// These changing means the derivation path, the coin type, the purpose, or the
/// address type changed. Any of those is a wallet-breaking change that must be
/// deliberate: a user who recovers with these words in another wallet would no
/// longer find their coins.
#[test]
fn the_standard_phrase_derives_pinned_addresses() {
    let mnemonic = test_mnemonic();

    let expected: &[(BitcoinNetwork, [&str; 3])] = &[
        (
            BitcoinNetwork::Testnet4,
            [
                "tb1q6rz28mcfaxtmd6v789l9rrlrusdprr9pqcpvkl",
                "tb1qd7spv5q28348xl4myc8zmh983w5jx32cjhkn97",
                "tb1qxdyjf6h5d6qxap4n2dap97q4j5ps6ua8sll0ct",
            ],
        ),
        (
            BitcoinNetwork::Signet,
            [
                "tb1q6rz28mcfaxtmd6v789l9rrlrusdprr9pqcpvkl",
                "tb1qd7spv5q28348xl4myc8zmh983w5jx32cjhkn97",
                "tb1qxdyjf6h5d6qxap4n2dap97q4j5ps6ua8sll0ct",
            ],
        ),
        (
            BitcoinNetwork::Regtest,
            [
                "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk",
                "bcrt1qd7spv5q28348xl4myc8zmh983w5jx32cs707jh",
                "bcrt1qxdyjf6h5d6qxap4n2dap97q4j5ps6ua8jkxz0z",
            ],
        ),
    ];

    for (network, addresses) in expected {
        for (index, expected_address) in addresses.iter().enumerate() {
            let actual = receive_address(&mnemonic, "", *network, 0, 0, index as u32)
                .unwrap()
                .to_string();
            assert_eq!(
                &actual, expected_address,
                "{network} index {index} changed — this breaks recovery in other wallets"
            );
        }
    }
}

/// A passphrase produces a different wallet, and the same passphrase written
/// two ways produces the same one.
///
/// The second half is a regression test with history. The hand-written BIP-39
/// implementation this crate used to carry omitted NFKD normalisation, so
/// `é` typed as a combining accent silently derived a different wallet from
/// every other implementation. The library handles it; this makes sure a future
/// change cannot quietly stop calling the normalising variant.
#[test]
fn passphrases_change_the_wallet_and_normalise_consistently() {
    let mnemonic = test_mnemonic();
    let network = BitcoinNetwork::Testnet4;

    let plain = receive_address(&mnemonic, "", network, 0, 0, 0).unwrap();
    let with_passphrase = receive_address(&mnemonic, "TREZOR", network, 0, 0, 0).unwrap();
    assert_ne!(plain.to_string(), with_passphrase.to_string());

    assert_eq!(
        with_passphrase.to_string(),
        "tb1qfwxzlmn42rh6wu6gt4f0yrfpe4gf0jtwgxagrr"
    );

    let combining = receive_address(&mnemonic, "e\u{301}clair", network, 0, 0, 0).unwrap();
    let precomposed = receive_address(&mnemonic, "\u{e9}clair", network, 0, 0, 0).unwrap();
    assert_eq!(
        combining.to_string(),
        precomposed.to_string(),
        "NFKD normalisation is not being applied"
    );
}

// --- derivation paths -------------------------------------------------------

/// The path is assembled from the registry's coin type, not a literal.
#[test]
fn bip84_paths_use_the_registrys_coin_type() {
    for network in BitcoinNetwork::ALL {
        let path = bip84_path(*network, 2, 1, 5).unwrap();
        assert_eq!(
            path.to_string(),
            format!("84'/{}'/2'/1/5", network.params().bip44_coin_type),
            "{network} path is not assembled from the registry"
        );
    }
}

#[test]
fn hardening_is_applied_to_the_first_three_levels_only() {
    let path = bip84_path(BitcoinNetwork::Testnet4, 0, 0, 0)
        .unwrap()
        .to_string();
    assert_eq!(path, "84'/1'/0'/0/0");
}

/// Account, change and index must reach the derivation rather than being
/// silently dropped — every distinct input gives a distinct address.
#[test]
fn every_path_component_changes_the_address() {
    let mnemonic = test_mnemonic();
    let network = BitcoinNetwork::Testnet4;

    let base = receive_address(&mnemonic, "", network, 0, 0, 0).unwrap();
    for (account, change, index) in [(1, 0, 0), (0, 1, 0), (0, 0, 1)] {
        let other = receive_address(&mnemonic, "", network, account, change, index).unwrap();
        assert_ne!(
            base.to_string(),
            other.to_string(),
            "account={account} change={change} index={index} did not reach the derivation"
        );
    }
}

// --- network separation (ARCHITECTURE.md §8) --------------------------------

/// An address always knows which network it is for, and parsing requires the
/// caller to say which network they believe they are on.
#[test]
fn addresses_carry_their_network() {
    let mnemonic = test_mnemonic();

    for network in BitcoinNetwork::ALL {
        let address = receive_address(&mnemonic, "", *network, 0, 0, 0).unwrap();
        assert_eq!(address.network(), *network);

        let reparsed = Address::parse(&address.to_string(), *network).unwrap();
        assert_eq!(reparsed, address);
    }
}

/// Regtest is the one test network its address prefix identifies.
#[test]
fn regtest_addresses_are_rejected_elsewhere() {
    let mnemonic = test_mnemonic();
    let regtest = receive_address(&mnemonic, "", BitcoinNetwork::Regtest, 0, 0, 0).unwrap();

    for network in [BitcoinNetwork::Testnet4, BitcoinNetwork::Signet] {
        assert!(
            Address::parse(&regtest.to_string(), network).is_err(),
            "a regtest address parsed as {network}"
        );
    }
}

/// The hazard from §8, stated as a passing test rather than a warning.
///
/// A testnet4 address *is* a valid signet address. Nothing in the string
/// distinguishes them, which is why the network is explicit state and must be
/// displayed next to every address in the UI.
#[test]
fn testnet4_and_signet_addresses_are_indistinguishable() {
    let mnemonic = test_mnemonic();

    let testnet4 = receive_address(&mnemonic, "", BitcoinNetwork::Testnet4, 0, 0, 0).unwrap();
    let signet = receive_address(&mnemonic, "", BitcoinNetwork::Signet, 0, 0, 0).unwrap();
    assert_eq!(testnet4.to_string(), signet.to_string());

    let as_signet = Address::parse(&testnet4.to_string(), BitcoinNetwork::Signet)
        .expect("§8: the same string is valid on both");
    assert_ne!(
        as_signet.network(),
        testnet4.network(),
        "same string, different network — this is why network is explicit state"
    );
}

// --- input validation -------------------------------------------------------

#[test]
fn malformed_phrases_are_rejected() {
    for phrase in [
        "",
        "abandon",
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon",
        "notaword abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    ] {
        assert!(parse_mnemonic(phrase).is_err(), "accepted {phrase:?}");
    }
}

#[test]
fn malformed_entropy_is_rejected() {
    for length in [0usize, 15, 17, 33] {
        assert!(
            mnemonic_from_entropy(&vec![0u8; length]).is_err(),
            "accepted {length} bytes of entropy"
        );
    }
    for length in [16usize, 20, 24, 28, 32] {
        assert!(mnemonic_from_entropy(&vec![0u8; length]).is_ok());
    }
}

#[test]
fn malformed_addresses_are_rejected() {
    for input in ["", "not an address", "tb1", "tb1qinvalidchecksum0000000000000000000000"] {
        assert!(
            Address::parse(input, BitcoinNetwork::Testnet4).is_err(),
            "accepted {input:?}"
        );
    }
}
