//! Enforcement of README §2: a user can never be on a network they did not
//! choose.
//!
//! # These tests replaced a set that had stopped meaning anything
//!
//! The previous file asserted that no mainnet constant existed anywhere. That
//! was the old §2, from when Thadam was testnet-only. It was a good test right
//! up until mainnet was added — at which point it would have been *deleted or
//! inverted*, and whatever replaced it needed to guard the new rule rather
//! than nothing at all.
//!
//! The new rule is narrower and harder: mainnet is a supported network, so the
//! danger is no longer that it exists. It is that a user ends up on it without
//! having said so.

use std::collections::BTreeSet;

use thadam_registry::{BitcoinNetwork, ChainId, EnvironmentId, ENVIRONMENTS};

/// There is no default network, and there must never be one.
///
/// A `Default` implementation is a network chosen by the code. This test
/// cannot check for the *absence* of a trait impl at runtime, so it does the
/// next best thing: it asserts the catalogue has no single distinguished
/// entry that a future `Default` would naturally return, and documents the
/// rule at the place someone would look.
///
/// The real enforcement is that `BitcoinNetwork` derives no `Default` and the
/// source guard below forbids adding one.
#[test]
fn no_network_is_privileged_as_a_default() {
    assert!(
        ENVIRONMENTS.len() > 1,
        "a single-entry catalogue is a default in disguise"
    );

    // Every network must be reachable only by naming it.
    for network in BitcoinNetwork::ALL {
        let parsed: BitcoinNetwork = network
            .as_str()
            .parse()
            .expect("every network parses from its own name");
        assert_eq!(parsed, *network);
    }
}

/// Parsing is exact. Nothing fuzzy, no aliases, no case folding.
///
/// README §2: nothing external may change the network. A generous parser is
/// how a malformed deep link or a stray config value picks a chain. Every
/// input below is a plausible mistake or a plausible attack, and all of them
/// must fail rather than resolve to mainnet.
#[test]
fn only_exact_network_names_parse() {
    const MUST_NOT_PARSE: &[&str] = &[
        "MAINNET",
        "Mainnet",
        "main",
        "bitcoin",
        "Bitcoin",
        "livenet",
        "prod",
        "production",
        "mainnet ",
        " mainnet",
        "main net",
        "mainnet\n",
        "",
        "testnet",
        "testnet3",
        "TESTNET4",
    ];

    for input in MUST_NOT_PARSE {
        assert!(
            input.parse::<BitcoinNetwork>().is_err(),
            "{input:?} parsed into a network"
        );
    }

    // And the same through the full identifier, which is the form that would
    // arrive from persisted state or a link.
    for input in ["bitcoin:MAINNET", "bitcoin:main", "bitcoin:", "bitcoin"] {
        assert!(
            input.parse::<EnvironmentId>().is_err(),
            "{input:?} parsed into an environment id"
        );
    }

    // The one spelling that does work, so the test cannot pass vacuously.
    assert_eq!(
        "bitcoin:mainnet".parse::<EnvironmentId>(),
        Ok(EnvironmentId::Bitcoin(BitcoinNetwork::Mainnet))
    );
}

/// Mainnet and the test networks share no encoding constant.
///
/// If any of these ever matched, an address built for one network would be
/// valid on the other, and §2's guarantee would rest on nothing but care.
#[test]
fn mainnet_shares_no_constant_with_any_test_network() {
    let mainnet = BitcoinNetwork::Mainnet.params();

    for network in BitcoinNetwork::TEST_NETWORKS {
        let test = network.params();
        assert_ne!(mainnet.bech32_hrp, test.bech32_hrp, "{network} HRP");
        assert_ne!(
            mainnet.base58_p2pkh_version, test.base58_p2pkh_version,
            "{network} P2PKH version"
        );
        assert_ne!(
            mainnet.base58_wif_version, test.base58_wif_version,
            "{network} WIF version"
        );
        assert_ne!(
            mainnet.bip32_public_version, test.bip32_public_version,
            "{network} xpub version"
        );
        assert_ne!(
            mainnet.bip44_coin_type, test.bip44_coin_type,
            "{network} coin type"
        );
    }
}

/// Mainnet's constants are the published ones.
///
/// Now that mainnet is supported, these being *wrong* is the expensive
/// failure — a subtly wrong coin type derives a wallet no other software can
/// open, and the user's real money is stranded.
///
/// Sources: Bitcoin Core `CMainParams`, BIP-32, SLIP-0044.
#[test]
fn mainnet_constants_are_the_published_ones() {
    let p = BitcoinNetwork::Mainnet.params();
    assert_eq!(p.bech32_hrp, "bc");
    assert_eq!(p.base58_p2pkh_version, 0x00);
    assert_eq!(p.base58_p2sh_version, 0x05);
    assert_eq!(p.base58_wif_version, 0x80);
    assert_eq!(p.bip32_public_version, 0x0488_B21E);
    assert_eq!(p.bip32_private_version, 0x0488_ADE4);
    assert_eq!(p.bip44_coin_type, 0);
}

/// One phrase, two wallets, no overlap.
///
/// The coin type is the only thing separating a user's real funds from their
/// test funds. If these ever matched, testnet activity would derive the same
/// keys as mainnet.
#[test]
fn mainnet_and_test_networks_derive_at_different_coin_types() {
    assert_eq!(BitcoinNetwork::Mainnet.params().bip44_coin_type, 0);
    for network in BitcoinNetwork::TEST_NETWORKS {
        assert_eq!(network.params().bip44_coin_type, 1, "{network}");
    }
}

/// README §9's hazard, stated as a passing test.
///
/// testnet4 and signet remain indistinguishable by address. If this fails, one
/// of them has been given distinguishing parameters and the warnings written
/// around that collision need revisiting.
#[test]
fn testnet4_and_signet_are_still_indistinguishable() {
    assert_eq!(
        BitcoinNetwork::Testnet4.params(),
        BitcoinNetwork::Signet.params(),
        "testnet4 and signet no longer share an encoding; revisit README §9"
    );
}

/// Every network a user could be on is a distinct, named value.
#[test]
fn every_network_is_distinct_and_named() {
    let names: BTreeSet<&str> = BitcoinNetwork::ALL.iter().map(|n| n.as_str()).collect();
    assert_eq!(names.len(), BitcoinNetwork::ALL.len(), "duplicate name");

    let display: BTreeSet<&str> = BitcoinNetwork::ALL
        .iter()
        .map(|n| n.display_name())
        .collect();
    assert_eq!(display.len(), BitcoinNetwork::ALL.len(), "duplicate display name");

    for network in BitcoinNetwork::ALL {
        assert!(!network.display_name().is_empty());
    }
}

/// `carries_real_value` says what it should.
///
/// It exists for display and confirmation copy. Nothing may branch on it to
/// decide whether an action is permitted — that would make the safety of a
/// user action depend on a boolean rather than on their explicit choice.
#[test]
fn only_mainnet_carries_real_value() {
    assert!(BitcoinNetwork::Mainnet.carries_real_value());
    for network in BitcoinNetwork::TEST_NETWORKS {
        assert!(!network.carries_real_value(), "{network}");
    }
    assert_eq!(
        BitcoinNetwork::ALL.len(),
        BitcoinNetwork::TEST_NETWORKS.len() + 1,
        "a network was added without deciding whether it carries real value"
    );
}

/// The catalogue and the enum agree.
#[test]
fn the_catalogue_covers_every_network_exactly_once() {
    for network in BitcoinNetwork::ALL {
        let matches = ENVIRONMENTS
            .iter()
            .filter(|e| e.id == EnvironmentId::Bitcoin(*network))
            .count();
        assert_eq!(matches, 1, "{network} appears {matches} times");
    }
    assert_eq!(ENVIRONMENTS.len(), BitcoinNetwork::ALL.len());
    assert_eq!(ChainId::ALL.len(), 1, "Thadam is Bitcoin only");
}

/// This crate must not grow a `Default` for a network, nor a dependency.
///
/// A source-reading test is unusual. It is here because both rules are
/// architectural rather than functional, and a comment is something nobody is
/// obliged to obey.
#[test]
fn no_default_impl_and_no_dependencies() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    for entry in std::fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).expect("source");

        for (number, line) in source.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if code.contains("impl Default") || code.contains("derive(Default") {
                violations.push(format!("{name}:{}: {}", number + 1, line.trim()));
            }
            if code.contains("#[derive(") && code.contains("Default") {
                violations.push(format!("{name}:{}: {}", number + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "a default network was introduced:\n{}",
        violations.join("\n")
    );

    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("manifest");
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .lines()
        .filter(|l| {
            let l = l.trim();
            !l.is_empty() && !l.starts_with('#') && !l.starts_with('[')
        })
        .count();
    assert_eq!(deps, 0, "the registry gained a dependency");
}
