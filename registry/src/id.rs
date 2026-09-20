//! Chain and environment identifiers.

use core::fmt;
use core::str::FromStr;

use crate::bitcoin::BitcoinNetwork;

/// A chain this build knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChainId {
    /// Bitcoin.
    Bitcoin,
}

impl ChainId {
    /// Every variant of this enum.
    pub const ALL: &'static [ChainId] = &[ChainId::Bitcoin];

    /// The chain name used in a `chain:environment` identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            ChainId::Bitcoin => "bitcoin",
        }
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ChainId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bitcoin" => Ok(ChainId::Bitcoin),
            _ => Err(()),
        }
    }
}

/// A chain together with one of its environments.
///
/// Rendered as `chain:environment` — `bitcoin:testnet4`. That string is the
/// wire and persistence form; this type is what code passes around, so nothing
/// ever does string surgery on an identifier.
///
/// # This enum is the enforcement point for §2
///
/// `bitcoin:mainnet` is not rejected by a validation rule that could be
/// removed or bypassed. It is unconstructable: [`BitcoinNetwork`] has no
/// mainnet variant, so there is no value of this type that names mainnet, and
/// no input to [`FromStr`] that produces one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EnvironmentId {
    /// A Bitcoin network.
    Bitcoin(BitcoinNetwork),
}

impl EnvironmentId {
    /// The chain half of this identifier.
    pub const fn chain(self) -> ChainId {
        match self {
            EnvironmentId::Bitcoin(_) => ChainId::Bitcoin,
        }
    }

    /// The environment half of this identifier.
    pub const fn environment_name(self) -> &'static str {
        match self {
            EnvironmentId::Bitcoin(n) => n.as_str(),
        }
    }
}

impl fmt::Display for EnvironmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.chain().as_str(), self.environment_name())
    }
}

impl FromStr for EnvironmentId {
    type Err = ParseEnvironmentIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (chain, environment) = s
            .split_once(':')
            .ok_or(ParseEnvironmentIdError::Malformed)?;
        if environment.contains(':') {
            return Err(ParseEnvironmentIdError::Malformed);
        }

        let chain: ChainId = chain
            .parse()
            .map_err(|()| ParseEnvironmentIdError::UnknownChain(chain.to_owned()))?;

        match chain {
            ChainId::Bitcoin => environment
                .parse::<BitcoinNetwork>()
                .map(EnvironmentId::Bitcoin)
                .map_err(|()| ParseEnvironmentIdError::UnknownEnvironment {
                    chain,
                    environment: environment.to_owned(),
                }),
        }
    }
}

/// Why a string was not a valid [`EnvironmentId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseEnvironmentIdError {
    /// The input was not exactly one `chain:environment` pair.
    Malformed,
    /// The chain half names a chain this build does not have.
    UnknownChain(String),
    /// The environment half names an environment this build does not have for
    /// that chain.
    ///
    /// `bitcoin:mainnet` lands here, and always will.
    UnknownEnvironment {
        /// The chain that was recognised.
        chain: ChainId,
        /// The environment name that was not.
        environment: String,
    },
}

impl fmt::Display for ParseEnvironmentIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseEnvironmentIdError::Malformed => {
                f.write_str("expected exactly one 'chain:environment' pair")
            }
            ParseEnvironmentIdError::UnknownChain(chain) => {
                write!(f, "unknown chain '{chain}'")
            }
            ParseEnvironmentIdError::UnknownEnvironment { chain, environment } => {
                write!(f, "unknown {chain} environment '{environment}'")
            }
        }
    }
}

impl std::error::Error for ParseEnvironmentIdError {}
