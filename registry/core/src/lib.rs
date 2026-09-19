//! Thadam chain registry.
//!
//! The catalogue of chains and the environments within them. See
//! ARCHITECTURE.md §6.
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
//! ARCHITECTURE.md §2: mainnet is unrepresentable, not disabled. If the
//! bech32 HRP or the BIP-44 coin type were read from a config file at runtime,
//! then `"bc"` and `0` would be one file edit away from producing a mainnet
//! wallet — and §2's guarantee would be prose rather than a property of the
//! build. So they live here, in a closed enum with no mainnet variant, and
//! `tests/no_mainnet.rs` fails the build if a mainnet value ever appears.
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
