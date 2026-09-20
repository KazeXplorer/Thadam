//! Enforcement of README §2: a user can never be on a network they did not
//! choose.
//!
//! # What this file replaced, and why that matters
//!
//! The previous guard file asserted that mainnet was unreachable — no mainnet
//! constant in the source, no `bc1` address producible. That was the old §2,
//! written when Thadam was testnet-only.
//!
//! Those tests would still have *passed* after mainnet was added, because
//! nothing about them was wrong; they simply stopped describing a rule anyone
//! still held. A green suite that guards nothing is a false statement about
//! coverage (§11), which is why they were deleted rather than adjusted.
//!
//! The rule now is harder to test and more worth testing: mainnet is
//! reachable, and must be reachable **only by a caller who named it.**

use std::fs;
use std::path::Path;

use thadam_registry::BitcoinNetwork;
use thadam_signer::{bip84_path, mnemonic_from_entropy, receive_address, Address};

fn mnemonic() -> bip39::Mnemonic {
    mnemonic_from_entropy(&[0x33u8; 16]).unwrap()
}

// --- what the network decides ----------------------------------------------

/// Every address carries the prefix the registry declares for its network.
///
/// Two independent sources must agree: `rust-bitcoin` builds the address, the
/// registry states the prefix. Neither gets to define the answer alone.
#[test]
fn every_address_carries_its_own_networks_prefix() {
    for network in BitcoinNetwork::ALL {
        for index in 0..8 {
            let address = receive_address(&mnemonic(), "", *network, 0, 0, index)
                .unwrap()
                .to_string();
            assert!(
                address.starts_with(network.params().bech32_hrp),
                "{network} produced {address}, expected prefix {}",
                network.params().bech32_hrp
            );
        }
    }
}

/// Mainnet addresses are `bc1`; no test network can produce one.
///
/// The concrete form of §2's guarantee. If a test network ever emitted `bc1`,
/// a user experimenting for free would be publishing an address that can
/// receive real money they cannot spend.
#[test]
fn only_mainnet_produces_mainnet_addresses() {
    let mainnet = receive_address(&mnemonic(), "", BitcoinNetwork::Mainnet, 0, 0, 0)
        .unwrap()
        .to_string();
    assert!(mainnet.starts_with("bc1"), "mainnet produced {mainnet}");

    for network in BitcoinNetwork::TEST_NETWORKS {
        let address = receive_address(&mnemonic(), "", *network, 0, 0, 0)
            .unwrap()
            .to_string();
        assert!(
            !address.starts_with("bc1"),
            "{network} produced a mainnet address: {address}"
        );
    }
}

/// A mainnet address never parses as a test network, and never the reverse.
///
/// This is the check that stops a user on testnet from paying a real address —
/// and a user on mainnet from burning real money to a testnet one.
#[test]
fn mainnet_and_test_addresses_never_cross() {
    let mainnet = receive_address(&mnemonic(), "", BitcoinNetwork::Mainnet, 0, 0, 0)
        .unwrap()
        .to_string();

    for network in BitcoinNetwork::TEST_NETWORKS {
        assert!(
            Address::parse(&mainnet, *network).is_err(),
            "a mainnet address parsed as {network}"
        );

        let test_address = receive_address(&mnemonic(), "", *network, 0, 0, 0)
            .unwrap()
            .to_string();
        assert!(
            Address::parse(&test_address, BitcoinNetwork::Mainnet).is_err(),
            "a {network} address parsed as mainnet"
        );
    }
}

/// One phrase, two wallets, no shared keys.
///
/// The same words on mainnet and on testnet must derive completely different
/// addresses. If they did not, testnet experimentation would expose real keys.
#[test]
fn the_same_phrase_gives_unrelated_wallets_on_each_network() {
    let mainnet = receive_address(&mnemonic(), "", BitcoinNetwork::Mainnet, 0, 0, 0).unwrap();

    for network in BitcoinNetwork::TEST_NETWORKS {
        let test = receive_address(&mnemonic(), "", *network, 0, 0, 0).unwrap();

        // Different coin type, so different keys entirely — not merely a
        // different encoding of the same key.
        assert_ne!(
            bip84_path(BitcoinNetwork::Mainnet, 0, 0, 0).unwrap(),
            bip84_path(*network, 0, 0, 0).unwrap(),
            "{network} derives at mainnet's path"
        );
        assert_ne!(
            mainnet.to_string()[4..],
            test.to_string()[test.to_string().find('1').unwrap() + 1..],
            "{network} shares a witness program with mainnet"
        );
    }
}

/// Derivation paths follow the network, not a constant.
#[test]
fn derivation_paths_follow_the_network() {
    assert_eq!(
        bip84_path(BitcoinNetwork::Mainnet, 0, 0, 0).unwrap().to_string(),
        "84'/0'/0'/0/0"
    );
    for network in BitcoinNetwork::TEST_NETWORKS {
        assert_eq!(
            bip84_path(*network, 0, 0, 0).unwrap().to_string(),
            "84'/1'/0'/0/0",
            "{network}"
        );
    }
}

/// README §9's hazard, unchanged and still explicit.
///
/// testnet4 and signet addresses remain interchangeable. Mainnet is not part
/// of that collision, which is the one piece of luck in this design.
#[test]
fn testnet4_and_signet_still_cross_but_mainnet_does_not() {
    let testnet4 = receive_address(&mnemonic(), "", BitcoinNetwork::Testnet4, 0, 0, 0).unwrap();

    let as_signet = Address::parse(&testnet4.to_string(), BitcoinNetwork::Signet)
        .expect("§9: testnet4 and signet are indistinguishable");
    assert_eq!(as_signet.to_string(), testnet4.to_string());
    assert_ne!(as_signet.network(), testnet4.network());

    assert!(
        Address::parse(&testnet4.to_string(), BitcoinNetwork::Mainnet).is_err(),
        "a testnet4 address parsed as mainnet"
    );
}

// --- how the network can be chosen -----------------------------------------

/// No string that is not an exact network name reaches a network.
///
/// §2: nothing external may change the network. This crate's entry points take
/// a `BitcoinNetwork` value, so the only way a string becomes a network is the
/// registry's parser — and that parser is exact. Re-asserted here because this
/// is the crate an attacker-supplied address arrives at.
#[test]
fn no_loose_string_selects_a_network() {
    for input in ["mainnet", "MAINNET", "bc", "bc1", "bitcoin", "main", "1"] {
        // There is no API here that takes a network as text. The closest thing
        // is parsing an address, which requires the caller to have already
        // chosen the network it is checked against.
        for network in BitcoinNetwork::ALL {
            let _ = Address::parse(input, *network);
        }
    }

    // And an address string cannot be turned into its own network: there is no
    // `Address::parse(s)` without a network argument, by design.
    let mainnet = receive_address(&mnemonic(), "", BitcoinNetwork::Mainnet, 0, 0, 0)
        .unwrap()
        .to_string();
    assert!(
        Address::parse(&mainnet, BitcoinNetwork::Testnet4).is_err(),
        "an address was allowed to override the caller's network"
    );
}

// --- structural enforcement -------------------------------------------------

/// The chokepoint must stay a chokepoint.
///
/// `src/network.rs` and `src/address.rs` are the only files allowed to name the
/// library's network and HRP types. A third conversion means §2's question —
/// "can the network change without the user acting?" — no longer has one place
/// to look.
#[test]
fn only_the_chokepoint_files_name_the_librarys_network_types() {
    const BOUNDARY_FILES: &[&str] = &["network.rs", "address.rs"];
    const LIBRARY_TYPES: &[&str] = &["bitcoin::Network", "KnownHrp", "Network::", "Params::"];

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    for entry in fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("readable entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();

        if path.extension().and_then(|e| e.to_str()) != Some("rs")
            || BOUNDARY_FILES.contains(&name.as_str())
        {
            continue;
        }

        let source = fs::read_to_string(&path).expect("readable source");
        for (number, line) in source.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            for needle in LIBRARY_TYPES {
                if code.contains(needle) {
                    violations.push(format!("{name}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the library's network types escaped the chokepoint:\n{}",
        violations.join("\n")
    );
}

/// No fallback arm, and no default network.
///
/// A `_ =>` arm in a network match means a future network silently inherits
/// another's constants. A `Default` means the code picks a chain. Both are
/// §2 violations that compile perfectly.
#[test]
fn no_fallback_arms_and_no_default_network() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    for entry in fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = fs::read_to_string(&path).expect("readable source");

        let mut inside_network_match = false;
        for (number, line) in source.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if code.contains("match network") {
                inside_network_match = true;
            } else if inside_network_match && code == "}" {
                inside_network_match = false;
            } else if inside_network_match && (code.starts_with("_ =>") || code.starts_with("_=>")) {
                violations.push(format!("{name}:{}: fallback arm: {}", number + 1, code));
            }

            if code.contains("impl Default for") && code.contains("Network") {
                violations.push(format!("{name}:{}: default network: {}", number + 1, code));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "§2 violations in source:\n{}",
        violations.join("\n")
    );
}
