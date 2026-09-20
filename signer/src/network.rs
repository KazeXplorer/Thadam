//! The network chokepoint.
//!
//! README §2: a user can never be on a network they did not choose. This
//! module is where Thadam's network type becomes the library's, and it is the
//! only place that conversion happens.
//!
//! # Why a chokepoint rather than a ban
//!
//! An earlier version of this file existed to make mainnet *unreachable* —
//! Thadam was testnet-only, and the rule was that mainnet parameters must not
//! be selectable at all. Mainnet is now a supported network, so that rule is
//! gone. What replaced it is narrower: mainnet is reachable, but only by a
//! caller who named it.
//!
//! A single conversion point is what makes that checkable. With one door, the
//! question "can the network change without the user acting?" has one place to
//! look. With conversions scattered across three crates it has none.
//!
//! # Rules for this module
//!
//! - Nothing outside this module and `address.rs` may name [`bitcoin::Network`]
//!   or `KnownHrp`.
//! - No fallback, no default, no `_ =>` arm. Every match here is exhaustive
//!   over [`BitcoinNetwork`], so a new network cannot be added without a
//!   deliberate decision at this line.
//! - Nothing here may infer a network from an address, a URL or stored state.
//!   The conversion takes a network; it never produces one.

use bitcoin::Network;
use thadam_registry::BitcoinNetwork;

/// Converts one of Thadam's networks to the library's.
///
/// `Network::Signet` stands in for testnet4. README §9: testnet4, testnet3 and
/// signet share every encoding constant, so the choice is invisible in any
/// address or key this crate produces. It would matter only to code needing
/// consensus rules rather than encodings, and nothing here does.
pub(crate) fn to_library_network(network: BitcoinNetwork) -> Network {
    match network {
        BitcoinNetwork::Mainnet => Network::Bitcoin,
        BitcoinNetwork::Testnet4 => Network::Signet,
        BitcoinNetwork::Signet => Network::Signet,
        BitcoinNetwork::Regtest => Network::Regtest,
    }
}

/// The address prefix of each network, as the registry states it.
///
/// Re-exported so guard tests can assert that what this crate *produces*
/// carries the prefix the registry *declares*, without either side being able
/// to define the answer on its own.
pub fn expected_hrp(network: BitcoinNetwork) -> &'static str {
    network.params().bech32_hrp
}
