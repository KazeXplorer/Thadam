//! One entry in the catalogue.

use core::fmt;

use crate::id::EnvironmentId;

/// Whether an environment is a shared network or one running on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NetworkKind {
    /// A shared network with other participants and real block production.
    Live,
    /// A network running locally, with no shared history.
    Local,
}

impl NetworkKind {
    /// The lowercase name used in the conformance fixture.
    pub const fn as_str(self) -> &'static str {
        match self {
            NetworkKind::Live => "live",
            NetworkKind::Local => "local",
        }
    }
}

impl fmt::Display for NetworkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A chain environment and the facts about it that are not chain parameters.
///
/// Chain parameters hang off the chain-specific network type
/// ([`crate::BitcoinNetwork::params`]) rather than living here, because they
/// have nothing in common across chains — see ARCHITECTURE.md §6. Flattening
/// them into one struct is the cross-chain abstraction the design refuses to
/// write before there are two implementations to extract it from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Environment {
    /// This environment's identifier.
    pub id: EnvironmentId,
    /// A name for a human, shown next to every address so the network is
    /// never inferred from the address itself.
    pub display_name: &'static str,
    /// Whether this is a shared or a local network.
    pub kind: NetworkKind,
}
