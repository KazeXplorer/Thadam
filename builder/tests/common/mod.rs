//! Shared fixtures.
//!
//! Coins are fabricated rather than fetched — this crate has no network access
//! by design (§3), and a builder that needed a chain to be tested would have
//! failed that requirement. §10 rejects fake chain responses in *production
//! paths*; test inputs are not one.

use bitcoin::hashes::Hash;
use bitcoin::{Amount, FeeRate, OutPoint, Txid};
use thadam_registry::BitcoinNetwork;
use thadam_signer::{mnemonic_from_entropy, receive_address, Address};
use thadam_builder::{Intent, Utxo};

/// A deterministic address for the given index, on the given network.
pub fn address(network: BitcoinNetwork, index: u32) -> Address {
    let mnemonic = mnemonic_from_entropy(&[0x21u8; 16]).unwrap();
    receive_address(&mnemonic, "", network, 0, 0, index).unwrap()
}

/// A coin worth `sats`, with a distinct outpoint per `seed`.
pub fn utxo(network: BitcoinNetwork, seed: u8, sats: u64) -> Utxo {
    Utxo {
        outpoint: OutPoint {
            txid: Txid::from_byte_array([seed; 32]),
            vout: u32::from(seed),
        },
        value: Amount::from_sat(sats),
        address: address(network, u32::from(seed) + 100),
    }
}

/// Several coins, one per value.
pub fn utxos(network: BitcoinNetwork, values: &[u64]) -> Vec<Utxo> {
    values
        .iter()
        .enumerate()
        .map(|(index, &sats)| utxo(network, index as u8 + 1, sats))
        .collect()
}

/// An intent paying `sats` at `sat_per_vb`.
pub fn intent(network: BitcoinNetwork, sats: u64, sat_per_vb: u64) -> Intent {
    Intent {
        recipient: address(network, 0),
        amount: Amount::from_sat(sats),
        change_address: address(network, 1),
        fee_rate: FeeRate::from_sat_per_vb(sat_per_vb).expect("valid fee rate"),
    }
}
