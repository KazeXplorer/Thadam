//! README §2 applied to this crate.
//!
//! The signer has its own guard. This one exists because `review.rs` converts
//! to the library's `Params` — a second conversion point, and therefore a
//! second place a network can be translated wrongly.
//!
//! Like the signer's, this file replaced one that asserted the *old* §2
//! ("mainnet is unreachable"). Those tests would have kept passing after
//! mainnet was added while guarding nothing.

use std::fs;
use std::path::Path;

use bitcoin::Amount;
use thadam_registry::BitcoinNetwork;
use thadam_signer::{mnemonic_from_entropy, receive_address};

mod common;
use common::{intent, utxos};

/// Every output carries the prefix its network declares.
#[test]
fn built_outputs_carry_their_networks_prefix() {
    for network in BitcoinNetwork::ALL {
        let payload = thadam_builder::build(
            &utxos(*network, &[5_000_000, 1_000_000]),
            &intent(*network, 250_000, 7),
        )
        .expect("must build");

        assert_eq!(payload.network, *network);

        for output in &payload.review.outputs {
            let rendered = output
                .address
                .as_ref()
                .map(|a| a.to_string())
                .unwrap_or_default();
            assert!(
                rendered.starts_with(network.params().bech32_hrp),
                "{network} output {rendered} does not carry the registry's prefix"
            );
        }
    }
}

/// Only a transaction built for mainnet may contain a mainnet address.
#[test]
fn only_mainnet_transactions_contain_mainnet_addresses() {
    for network in BitcoinNetwork::TEST_NETWORKS {
        let payload =
            thadam_builder::build(&utxos(*network, &[5_000_000]), &intent(*network, 250_000, 7))
                .expect("must build");

        for output in &payload.review.outputs {
            let rendered = output
                .address
                .as_ref()
                .map(|a| a.to_string())
                .unwrap_or_default();
            assert!(
                !rendered.starts_with("bc1"),
                "{network} produced a mainnet output: {rendered}"
            );
        }
    }

    let mainnet = thadam_builder::build(
        &utxos(BitcoinNetwork::Mainnet, &[5_000_000]),
        &intent(BitcoinNetwork::Mainnet, 250_000, 7),
    )
    .expect("must build");
    assert!(mainnet.review.outputs.iter().all(|o| o
        .address
        .as_ref()
        .is_some_and(|a| a.to_string().starts_with("bc1"))));
}

/// Real money and test coins can never appear in one transaction.
///
/// The most expensive mistake this crate could make. A mainnet coin spent in a
/// transaction whose recipient is a testnet address is real money sent to an
/// address nobody controls.
#[test]
fn mainnet_and_test_networks_never_mix() {
    for network in BitcoinNetwork::TEST_NETWORKS {
        // Mainnet coins, test recipient.
        assert!(
            thadam_builder::build(
                &utxos(BitcoinNetwork::Mainnet, &[5_000_000]),
                &intent(*network, 100_000, 5)
            )
            .is_err(),
            "built a transaction spending mainnet coins to a {network} address"
        );

        // Test coins, mainnet recipient.
        assert!(
            thadam_builder::build(
                &utxos(*network, &[5_000_000]),
                &intent(BitcoinNetwork::Mainnet, 100_000, 5)
            )
            .is_err(),
            "built a transaction spending {network} coins to a mainnet address"
        );
    }
}

/// A single foreign coin among good ones is refused.
#[test]
fn one_foreign_coin_is_enough_to_refuse() {
    let mnemonic = mnemonic_from_entropy(&[0x21u8; 16]).unwrap();

    let mut mixed = utxos(BitcoinNetwork::Mainnet, &[5_000_000, 5_000_000]);
    mixed[1].address = receive_address(&mnemonic, "", BitcoinNetwork::Testnet4, 0, 0, 102).unwrap();

    assert!(
        thadam_builder::build(&mixed, &intent(BitcoinNetwork::Mainnet, 100_000, 5)).is_err(),
        "a testnet coin was accepted into a mainnet transaction"
    );
}

/// README §9 where it bites hardest: identical strings, different networks.
///
/// A testnet4 coin and a signet coin have the same address text. The type is
/// the only thing that separates them, and it must.
#[test]
fn testnet4_and_signet_are_separated_by_type_not_by_address() {
    let mut mixed = utxos(BitcoinNetwork::Testnet4, &[5_000_000, 5_000_000]);
    let mnemonic = mnemonic_from_entropy(&[0x21u8; 16]).unwrap();
    let signet_coin = receive_address(&mnemonic, "", BitcoinNetwork::Signet, 0, 0, 102).unwrap();

    assert_eq!(signet_coin.to_string(), mixed[1].address.to_string());
    assert_ne!(signet_coin.network(), mixed[1].address.network());

    mixed[1].address = signet_coin;
    assert!(
        thadam_builder::build(&mixed, &intent(BitcoinNetwork::Testnet4, 100_000, 5)).is_err(),
        "identical strings, different networks — the type must still separate them"
    );
}

/// The fee never exceeds the inputs, on any network.
#[test]
fn the_fee_never_exceeds_the_inputs() {
    for network in BitcoinNetwork::ALL {
        let payload =
            thadam_builder::build(&utxos(*network, &[400_000]), &intent(*network, 300_000, 20))
                .expect("must build");

        assert!(payload.review.fee < payload.review.total_input);
        assert!(payload.review.fee > Amount::ZERO);
    }
}

// --- structural enforcement -------------------------------------------------

/// No fallback arm in any network match, and no default network.
///
/// A `_ =>` arm means a future network silently inherits another's parameters.
/// On mainnet that is real money encoded for the wrong chain.
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
