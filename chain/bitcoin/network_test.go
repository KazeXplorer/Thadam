package bitcoin_test

import (
	"testing"

	"thadam/chain/bitcoin"
)

// These tests pin the compiled catalogue. Adding or removing a network here is
// a deliberate change that requires an architectural decision — not a refactor.

func TestParseNetwork_KnownNetworks(t *testing.T) {
	for _, id := range []string{"mainnet", "testnet4", "signet", "regtest"} {
		n, err := bitcoin.ParseNetwork(id)
		if err != nil {
			t.Errorf("ParseNetwork(%q) returned unexpected error: %v", id, err)
			continue
		}
		if string(n) != id {
			t.Errorf("ParseNetwork(%q) = %q, want %q", id, string(n), id)
		}
	}
}

func TestParseNetwork_UnknownNetworksRefused(t *testing.T) {
	// These must never parse: aliases, other chains, empty string, typos.
	for _, id := range []string{"", "bitcoin", "main", "MAINNET", "testnet", "testnet3", "sepolia", "devnet", "unknown"} {
		_, err := bitcoin.ParseNetwork(id)
		if err == nil {
			t.Errorf("ParseNetwork(%q) succeeded — expected an error for an unknown network", id)
		}
	}
}

// All() must return exactly the compiled catalogue — no more, no less.
func TestAll_ReturnsExactCatalogue(t *testing.T) {
	all := bitcoin.All()
	seen := make(map[bitcoin.Network]bool)
	for _, n := range all {
		if seen[n] {
			t.Errorf("All() returned duplicate: %q", n)
		}
		seen[n] = true
	}

	for _, expected := range []bitcoin.Network{bitcoin.Mainnet, bitcoin.Testnet4, bitcoin.Signet, bitcoin.Regtest} {
		if !seen[expected] {
			t.Errorf("All() did not include %q", expected)
		}
	}

	if len(all) != 4 {
		t.Errorf("All() returned %d networks, want exactly 4 (mainnet, testnet4, signet, regtest)", len(all))
	}
}

// ParseNetwork round-trips: the string form of a parsed Network must parse
// back to the same value.
func TestParseNetwork_RoundTrip(t *testing.T) {
	for _, n := range bitcoin.All() {
		reparsed, err := bitcoin.ParseNetwork(string(n))
		if err != nil {
			t.Errorf("ParseNetwork(string(%q)) failed: %v", n, err)
			continue
		}
		if reparsed != n {
			t.Errorf("round-trip: got %q, want %q", reparsed, n)
		}
	}
}
