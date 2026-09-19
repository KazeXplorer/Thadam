//! The mainnet chokepoint.
//!
//! ARCHITECTURE.md §2. This module exists because of one unavoidable fact:
//! **every standard Bitcoin library knows about mainnet.** Linking
//! `rust-bitcoin` puts `Network::Bitcoin`, the `bc` prefix and coin type 0 into
//! the binary, so §2's original wording — "those values do not exist in the
//! binary" — stopped being literally true the day this crate started using
//! audited code instead of hand-written code.
//!
//! That was the right trade. Unreviewed cryptography written for a project
//! with real users is a worse risk than a constant sitting unused in a
//! dependency. But the guarantee had to be restated honestly rather than
//! quietly weakened, so §2 now reads:
//!
//! > No Thadam type can name mainnet, and exactly one function converts to the
//! > library's network type.
//!
//! That function is [`to_library_network`]. It is the only door, it is an
//! exhaustive match over an enum with no mainnet variant, and
//! `tests/mainnet_guard.rs` asserts that nothing reachable through this crate's
//! public API can produce a mainnet address.
//!
//! # Rules for this module
//!
//! - Nothing outside this module may name [`bitcoin::Network`].
//! - Nothing may add a fallback, a default, or a `_ =>` arm below.
//! - If a second conversion function appears anywhere, the chokepoint is gone
//!   and §2 is no longer enforced by anything.

use bitcoin::Network;
use thadam_registry::BitcoinNetwork;

/// Converts one of Thadam's networks to the library's.
///
/// The match is exhaustive over [`BitcoinNetwork`], which has no mainnet
/// variant, so there is no input to this function that returns
/// `Network::Bitcoin`. Adding a mainnet variant upstream would make this
/// function stop compiling — which is the point, and is the same mechanism the
/// registry uses.
///
/// `Network::Signet` stands in for testnet4. ARCHITECTURE.md §8: testnet4,
/// testnet3 and signet share every encoding constant, so the choice is
/// invisible in any address or key this crate produces. It matters only if a
/// caller ever needs consensus rules rather than encodings, which nothing here
/// does.
pub(crate) fn to_library_network(network: BitcoinNetwork) -> Network {
    match network {
        BitcoinNetwork::Testnet4 => Network::Signet,
        BitcoinNetwork::Signet => Network::Signet,
        BitcoinNetwork::Regtest => Network::Regtest,
    }
}

/// The address prefix mainnet would use, recorded here only so its absence can
/// be asserted. Never construct an address with this.
pub const FORBIDDEN_MAINNET_HRP: &str = "bc";
