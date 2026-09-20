//! Thadam chain registry.
//!
//! The catalogue of chains and the environments within them. See
//! README §7.
//!
//! # What this crate is
//!
//! Lookup only. It answers "which environments exist" and "what are this
//! environment's parameters". It does not know how to derive a key, encode an
//! address, build a transaction, or reach a network. The moment chain logic
//! enters here it has become the premature abstraction the design avoids.
//!
//! # Why the constants are compiled in
//!
//! README §2: a user can never be on a network they did not choose. If the
//! bech32 HRP or the BIP-44 coin type were read from a config file at runtime,
//! a user's network would be decided by whatever wrote that file last. They
//! live here instead, in a closed enum with no `Default`, and
//! `tests/network_safety.rs` fails the build if that changes.
//!
//! # What is deliberately absent
//!
//! RPC and explorer endpoints. Those are operational configuration: they vary
//! per deployment, they change, and getting one wrong misroutes a request
//! rather than silently re-chaining a signature. They are supplied at runtime
//! and validated against this catalogue, not compiled into it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bitcoin;
mod environment;
mod id;

pub use bitcoin::{BitcoinNetwork, BitcoinParams};
pub use environment::{Environment, NetworkKind};
pub use id::{ChainId, EnvironmentId, ParseEnvironmentIdError};

/// Every environment this build knows about.
///
/// This is the whole catalogue. An environment that is not in this slice
/// cannot be selected, because [`EnvironmentId`] cannot be constructed for it.
pub const ENVIRONMENTS: &[Environment] = &[
    Environment {
        id: EnvironmentId::Bitcoin(BitcoinNetwork::Mainnet),
        display_name: "Bitcoin MAINNET",
        kind: NetworkKind::Live,
    },
    Environment {
        id: EnvironmentId::Bitcoin(BitcoinNetwork::Testnet4),
        display_name: "Bitcoin testnet4",
        kind: NetworkKind::Live,
    },
    Environment {
        id: EnvironmentId::Bitcoin(BitcoinNetwork::Signet),
        display_name: "Bitcoin signet",
        kind: NetworkKind::Live,
    },
    Environment {
        id: EnvironmentId::Bitcoin(BitcoinNetwork::Regtest),
        display_name: "Bitcoin regtest",
        kind: NetworkKind::Local,
    },
];

/// Look up an environment by its identifier.
///
/// Returns `None` only if the catalogue and [`EnvironmentId`] have drifted
/// apart, which `tests/catalogue.rs` asserts cannot happen.
pub fn find(id: EnvironmentId) -> Option<&'static Environment> {
    ENVIRONMENTS.iter().find(|e| e.id == id)
}
