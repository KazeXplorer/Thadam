//! Enforcement of ARCHITECTURE.md §2, in its post-swap form.
//!
//! §2 used to say mainnet parameters "do not exist in the binary". That was
//! true while this crate hand-rolled its own BIP implementations. It stopped
//! being true when it started using `rust-bitcoin`, because every standard
//! Bitcoin library knows about mainnet and there is no testnet-only one.
//!
//! The guarantee is now narrower and stated honestly:
//!
//! > No Thadam type can name mainnet, and exactly one function converts to the
//! > library's network type.
//!
//! Narrower means it needs *more* enforcement, not less. This file provides it
//! two ways: behaviourally, by exhausting the public API and checking nothing
//! mainnet-shaped comes out; and structurally, by reading the source and
//! failing if the chokepoint has been bypassed.

use std::fs;
use std::path::Path;

use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use thadam_registry::BitcoinNetwork;
use thadam_signer::network::FORBIDDEN_MAINNET_HRP;
use thadam_signer::{mnemonic_from_entropy, receive_address, Address};

/// No address this crate can produce may be a mainnet address.
///
/// Exhausts every network in the registry across many phrases and indices. A
/// mainnet address would begin `bc1`, which is the one thing a user could not
/// recover from.
#[test]
fn no_reachable_code_path_produces_a_mainnet_address() {
    let mut rng = StdRng::seed_from_u64(0x2_0000);

    for network in BitcoinNetwork::ALL {
        for _ in 0..64 {
            let mut entropy = [0u8; 16];
            rng.fill_bytes(&mut entropy);
            let mnemonic = mnemonic_from_entropy(&entropy).unwrap();

            for (account, change, index) in [(0, 0, 0), (0, 1, 7), (3, 0, 999)] {
                let address = receive_address(&mnemonic, "", *network, account, change, index)
                    .unwrap()
                    .to_string();

                assert!(
                    !address.starts_with(&format!("{FORBIDDEN_MAINNET_HRP}1")),
                    "{network} produced a mainnet address: {address}"
                );
                assert!(
                    address.starts_with(network.params().bech32_hrp),
                    "{network} address {address} does not carry the registry's prefix"
                );
            }
        }
    }
}

/// A mainnet address must not parse as any network this build knows about.
#[test]
fn mainnet_addresses_do_not_parse_on_any_network() {
    // A well-formed mainnet P2WPKH address. Present only to be rejected.
    const MAINNET_ADDRESS: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

    for network in BitcoinNetwork::ALL {
        assert!(
            Address::parse(MAINNET_ADDRESS, *network).is_err(),
            "{network} accepted a mainnet address"
        );
    }
}

/// The derivation path may never use mainnet's SLIP-0044 coin type.
#[test]
fn no_network_derives_at_mainnet_coin_type() {
    for network in BitcoinNetwork::ALL {
        let path = thadam_signer::bip84_path(*network, 0, 0, 0).unwrap();
        let rendered = path.to_string();
        assert!(
            rendered.starts_with("84'/1'/"),
            "{network} derives at {rendered}, which is not a test-network path"
        );
        assert_ne!(network.params().bip44_coin_type, 0);
    }
}

// --- structural enforcement -------------------------------------------------

/// The chokepoint must stay a chokepoint.
///
/// `src/network.rs` and `src/address.rs` are the only files allowed to name the
/// library's network and HRP types. If a third file starts doing conversions,
/// §2 is no longer enforced by anything, and the behavioural tests above would
/// not necessarily notice — they only cover the paths that exist today.
///
/// A source-reading test is unusual. It is here because the rule it protects is
/// architectural rather than functional, and the alternative is a comment
/// nobody is obliged to obey.
#[test]
fn only_the_chokepoint_files_name_the_librarys_network_types() {
    const BOUNDARY_FILES: &[&str] = &["network.rs", "address.rs"];
    const LIBRARY_TYPES: &[&str] = &["bitcoin::Network", "KnownHrp", "Network::"];

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
            // Doc comments discuss these types on purpose.
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

/// No source file may name mainnet's constants.
///
/// The registry has the same rule for the same reason. Here it catches a
/// developer reaching for `Network::Bitcoin` or `KnownHrp::Mainnet` inside the
/// boundary files themselves, where the previous test deliberately stops
/// looking.
#[test]
fn no_source_file_names_a_mainnet_constant() {
    const FORBIDDEN: &[&str] = &[
        "Network::Bitcoin",
        "KnownHrp::Mainnet",
        "bip44_coin_type: 0",
    ];

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();

    for entry in fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = fs::read_to_string(&path).expect("readable source");

        for (number, line) in source.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            for needle in FORBIDDEN {
                if code.contains(needle) {
                    violations.push(format!("{name}:{}: {}", number + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "mainnet constants named in source:\n{}",
        violations.join("\n")
    );
}
