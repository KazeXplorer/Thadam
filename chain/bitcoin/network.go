// Package bitcoin holds the compiled catalogue of Bitcoin networks and the
// types that carry a network through the chain service.
//
// ARCHITECTURE.md §2: network is explicit state, carried in types, never
// inferred. Nothing external can change the network. These constants are
// compiled in; they do not come from config, environment variables, or URL
// parameters. If a caller supplies a network ID that is not in this catalogue,
// the call is refused at the boundary rather than silently wrong later.
package bitcoin

import "fmt"

// Network identifies a Bitcoin network.
//
// This type is the service's equivalent of the Rust registry's BitcoinNetwork.
// It is a string internally so that it round-trips through JSON and log lines
// without a separate encoding step, but it is never constructed from raw input:
// every value arrives through [ParseNetwork], which rejects anything not in the
// catalogue.
type Network string

const (
	// Mainnet is Bitcoin mainnet (production).
	//
	// The chain service holds no keys and does not sign — it only relays UTXO
	// queries, fee estimates, and signed transactions. Mainnet is therefore safe
	// to enable here. The wallet-side mainnet gates (entropy review, RBF, guard
	// tests) remain in the mobile app; they do not apply to this relay service.
	//
	// Mainnet address prefixes: bc1p (P2TR, bech32m), bc1q (P2WPKH, bech32).
	Mainnet Network = "mainnet"

	// Testnet4 is Bitcoin testnet4.
	//
	// Address prefix: tb1p (P2TR, bech32m), tb1q (P2WPKH, bech32).
	// Shares its prefix with Signet — the UI must name the network in words
	// alongside every address (§2 of ARCHITECTURE.md).
	Testnet4 Network = "testnet4"

	// Signet is Bitcoin signet. Used for integration testing and staging.
	// Same address prefix as Testnet4; the collision is documented in §9.
	Signet Network = "signet"

	// Regtest is a local Bitcoin regtest network. Used in development and CI.
	// Distinctive prefix: bcrt1.
	Regtest Network = "regtest"
)

// catalogue is the set of network identifiers this binary recognises.
//
// Runtime config is validated against this set at load time (config.Config.Validate).
// A network absent here cannot be configured.
var catalogue = map[Network]bool{
	Mainnet:  true,
	Testnet4: true,
	Signet:   true,
	Regtest:  true,
}

// ParseNetwork parses a network identifier from external input.
//
// Returns an error if the string is not in the compiled catalogue. This is the
// only place in the service where a Network value is constructed from a string,
// so the catalogue check is enforced at every external boundary without a rule
// anyone must remember.
func ParseNetwork(s string) (Network, error) {
	n := Network(s)
	if !catalogue[n] {
		return "", fmt.Errorf("unknown Bitcoin network %q: known networks are mainnet, testnet4, signet, regtest", s)
	}
	return n, nil
}

// All returns all networks in the compiled catalogue, in a consistent order.
// Used for validation and documentation; the order is deterministic but not
// significant.
func All() []Network {
	return []Network{Mainnet, Testnet4, Signet, Regtest}
}
