//! Address encoding. The point where the registry's constants get used.

use bitcoin::{CompressedPublicKey, KnownHrp};
use thadam_registry::BitcoinNetwork;

use crate::network::to_library_network;
use crate::Error;

/// A P2WPKH address, together with the network it belongs to.
///
/// The network is carried in the type, not inferred from the string.
/// ARCHITECTURE.md §8: `tb1…` is testnet3, testnet4 *and* signet, so an address
/// on its own is ambiguous and sending to the wrong network succeeds silently.
///
/// [`Display`](std::fmt::Display) renders only the address. Anything shown to a
/// user must show [`Address::network`] alongside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    address: String,
    network: BitcoinNetwork,
}

impl Address {
    /// Builds the P2WPKH address for a compressed public key on `network`.
    pub fn p2wpkh(
        public_key: &CompressedPublicKey,
        network: BitcoinNetwork,
    ) -> Result<Self, Error> {
        let hrp = known_hrp(network);
        let address = bitcoin::Address::p2wpkh(public_key, hrp);

        // The registry's HRP is not used to *build* the address — the library
        // owns that now — so it is checked against the result instead. If the
        // registry and the library ever disagree about what `tb` means, this
        // is where it surfaces, rather than in a user's lost coins.
        let encoded = address.to_string();
        let expected_hrp = network.params().bech32_hrp;
        if !encoded.starts_with(expected_hrp) {
            return Err(Error::HrpMismatch {
                expected: expected_hrp.to_owned(),
                produced: encoded,
            });
        }

        Ok(Address {
            address: encoded,
            network,
        })
    }

    /// Parses an address, checking it belongs to `network`.
    ///
    /// The network is an argument rather than a return value on purpose. There
    /// is no `Address::parse(s)` that works out which chain `s` is for, because
    /// for `tb1…` addresses no such function can exist.
    pub fn parse(address: &str, network: BitcoinNetwork) -> Result<Self, Error> {
        let parsed: bitcoin::Address<bitcoin::address::NetworkUnchecked> =
            address.parse().map_err(|_| Error::MalformedAddress)?;

        let checked = parsed
            .require_network(to_library_network(network))
            .map_err(|_| Error::WrongNetwork {
                expected: network,
                address: address.to_owned(),
            })?;

        Ok(Address {
            address: checked.to_string(),
            network,
        })
    }

    /// The network this address belongs to.
    pub fn network(&self) -> BitcoinNetwork {
        self.network
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.address)
    }
}

/// The library's HRP selector for one of our networks.
///
/// Like [`to_library_network`], this is an exhaustive match with no mainnet
/// arm. `KnownHrp::Mainnet` is nameable in the dependency and is not named
/// here, which is the whole of §2's remaining guarantee.
fn known_hrp(network: BitcoinNetwork) -> KnownHrp {
    match network {
        BitcoinNetwork::Testnet4 | BitcoinNetwork::Signet => KnownHrp::Testnets,
        BitcoinNetwork::Regtest => KnownHrp::Regtest,
    }
}
