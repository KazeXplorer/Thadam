//! The catalogue is complete, consistent, and round-trips.

use std::collections::BTreeSet;

use thadam_registry::{find, BitcoinNetwork, ChainId, EnvironmentId, ParseEnvironmentIdError,
                      ENVIRONMENTS};

#[test]
fn all_lists_every_bitcoin_variant() {
    // This match is exhaustive. Adding a variant to BitcoinNetwork makes this
    // file stop compiling, and the count below then forces a deliberate
    // decision about whether the new network belongs in ALL.
    for network in BitcoinNetwork::ALL {
        match network {
            BitcoinNetwork::Mainnet
            | BitcoinNetwork::Testnet4
            | BitcoinNetwork::Signet
            | BitcoinNetwork::Regtest => {}
        }
    }

    assert_eq!(BitcoinNetwork::ALL.len(), 4);

    let distinct: BTreeSet<_> = BitcoinNetwork::ALL.iter().collect();
    assert_eq!(distinct.len(), BitcoinNetwork::ALL.len(), "ALL has a duplicate");
}

#[test]
fn every_network_appears_in_the_catalogue_exactly_once() {
    for network in BitcoinNetwork::ALL {
        let matches = ENVIRONMENTS
            .iter()
            .filter(|e| e.id == EnvironmentId::Bitcoin(*network))
            .count();
        assert_eq!(matches, 1, "{network} appears {matches} times in ENVIRONMENTS");
    }

    assert_eq!(ENVIRONMENTS.len(), BitcoinNetwork::ALL.len());
}

#[test]
fn catalogue_ids_are_unique() {
    let distinct: BTreeSet<_> = ENVIRONMENTS.iter().map(|e| e.id).collect();
    assert_eq!(distinct.len(), ENVIRONMENTS.len());
}

#[test]
fn find_resolves_every_catalogued_id() {
    for environment in ENVIRONMENTS {
        assert_eq!(find(environment.id), Some(environment));
    }
}

#[test]
fn identifiers_round_trip_through_their_string_form() {
    for environment in ENVIRONMENTS {
        let rendered = environment.id.to_string();
        let parsed: EnvironmentId = rendered
            .parse()
            .unwrap_or_else(|e| panic!("{rendered} did not parse back: {e}"));
        assert_eq!(parsed, environment.id);
    }
}

#[test]
fn identifiers_render_as_chain_colon_environment() {
    assert_eq!(
        EnvironmentId::Bitcoin(BitcoinNetwork::Testnet4).to_string(),
        "bitcoin:testnet4"
    );
    assert_eq!(
        EnvironmentId::Bitcoin(BitcoinNetwork::Regtest).to_string(),
        "bitcoin:regtest"
    );
}

#[test]
fn malformed_identifiers_are_rejected() {
    for input in ["", "bitcoin", "testnet4", ":", "bitcoin:testnet4:extra"] {
        assert!(
            matches!(
                input.parse::<EnvironmentId>(),
                Err(ParseEnvironmentIdError::Malformed | ParseEnvironmentIdError::UnknownChain(_))
            ),
            "{input:?} was not rejected as malformed"
        );
    }
}

#[test]
fn unknown_chains_and_environments_report_which_half_failed() {
    assert_eq!(
        "ethereum:sepolia".parse::<EnvironmentId>(),
        Err(ParseEnvironmentIdError::UnknownChain("ethereum".to_owned()))
    );
    assert_eq!(
        "bitcoin:testnet3".parse::<EnvironmentId>(),
        Err(ParseEnvironmentIdError::UnknownEnvironment {
            chain: ChainId::Bitcoin,
            environment: "testnet3".to_owned(),
        })
    );
}

