package config_test

import (
	"os"
	"path/filepath"
	"testing"

	"gopkg.in/yaml.v3"

	"thadam/chain/config"
)

// writeConfig writes cfg as a YAML file in a temp dir and returns the path.
func writeConfig(t *testing.T, cfg config.Config) string {
	t.Helper()
	f, err := os.CreateTemp(t.TempDir(), "*.yaml")
	if err != nil {
		t.Fatal(err)
	}
	if err := yaml.NewEncoder(f).Encode(cfg); err != nil {
		t.Fatal(err)
	}
	f.Close()
	return f.Name()
}

func TestLoad_ValidConfig(t *testing.T) {
	cfg := config.Config{
		ListenAddr: ":9090",
		Networks: map[string]config.NetworkConfig{
			"testnet4": {ProviderURLs: []string{"https://mempool.space/testnet4/api"}},
		},
	}
	path := writeConfig(t, cfg)

	loaded, err := config.Load(path)
	if err != nil {
		t.Fatalf("Load returned unexpected error: %v", err)
	}
	if loaded.ListenAddr != ":9090" {
		t.Errorf("ListenAddr = %q, want %q", loaded.ListenAddr, ":9090")
	}
	nc, ok := loaded.Networks["testnet4"]
	if !ok {
		t.Fatal("testnet4 network missing from loaded config")
	}
	if len(nc.ProviderURLs) != 1 || nc.ProviderURLs[0] != "https://mempool.space/testnet4/api" {
		t.Errorf("testnet4 ProviderURLs = %v, want [https://mempool.space/testnet4/api]", nc.ProviderURLs)
	}
}

// The listen address must default to ":8080" when omitted.
func TestLoad_DefaultListenAddr(t *testing.T) {
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{
			"testnet4": {ProviderURLs: []string{"https://mempool.space/testnet4/api"}},
		},
	}
	path := writeConfig(t, cfg)

	loaded, err := config.Load(path)
	if err != nil {
		t.Fatalf("Load returned unexpected error: %v", err)
	}
	if loaded.ListenAddr != ":8080" {
		t.Errorf("default ListenAddr = %q, want :8080", loaded.ListenAddr)
	}
}

// A config with no networks configured must be rejected at load time.
func TestValidate_EmptyNetworksRejected(t *testing.T) {
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{},
	}
	path := writeConfig(t, cfg)

	_, err := config.Load(path)
	if err == nil {
		t.Error("Load with empty networks should have returned an error")
	}
}

// A network with zero provider URLs must be rejected.
func TestValidate_EmptyProviderURLsRejected(t *testing.T) {
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{
			"testnet4": {ProviderURLs: []string{}},
		},
	}
	path := writeConfig(t, cfg)

	_, err := config.Load(path)
	if err == nil {
		t.Error("Load with empty provider_urls should have returned an error")
	}
}

// Multiple provider URLs must all be validated.
func TestValidate_MultipleURLsAllValidated(t *testing.T) {
	// Good first URL, bad second URL — must fail.
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{
			"testnet4": {ProviderURLs: []string{
				"https://mempool.space/testnet4/api",
				"ftp://bad.example.com/api", // wrong scheme
			}},
		},
	}
	path := writeConfig(t, cfg)

	_, err := config.Load(path)
	if err == nil {
		t.Error("Load with a bad second provider URL should have returned an error")
	}
}

// Multiple valid URLs must all be accepted.
func TestValidate_MultipleValidURLsAccepted(t *testing.T) {
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{
			"testnet4": {ProviderURLs: []string{
				"https://mempool.space/testnet4/api",
				"https://blockstream.info/testnet/api",
			}},
		},
	}
	path := writeConfig(t, cfg)

	loaded, err := config.Load(path)
	if err != nil {
		t.Fatalf("Load returned unexpected error: %v", err)
	}
	if len(loaded.Networks["testnet4"].ProviderURLs) != 2 {
		t.Errorf("expected 2 provider URLs, got %d", len(loaded.Networks["testnet4"].ProviderURLs))
	}
}

// A network key not in the compiled catalogue must be rejected.
// This is the config-side half of the §2 guarantee: a config file naming
// an unrecognised network is refused before the process serves any requests.
func TestValidate_UnknownNetworkRejected(t *testing.T) {
	for _, badNetwork := range []string{"bitcoin", "main", "ethereum", "unknown"} {
		t.Run(badNetwork, func(t *testing.T) {
			cfg := config.Config{
				Networks: map[string]config.NetworkConfig{
					badNetwork: {ProviderURLs: []string{"https://example.com/api"}},
				},
			}
			path := writeConfig(t, cfg)

			_, err := config.Load(path)
			if err == nil {
				t.Errorf("Load with network %q should have returned an error", badNetwork)
			}
		})
	}
}

// mainnet is now a valid network identifier (the chain service holds no keys).
func TestValidate_MainnetAccepted(t *testing.T) {
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{
			"mainnet": {ProviderURLs: []string{"https://mempool.space/api"}},
		},
	}
	path := writeConfig(t, cfg)

	_, err := config.Load(path)
	if err != nil {
		t.Errorf("Load with mainnet returned unexpected error: %v", err)
	}
}

// Provider URLs must be valid http(s) URLs.
func TestValidate_InvalidURLsRejected(t *testing.T) {
	badURLs := []string{
		"",
		"not a url",
		"ftp://example.com/api",   // wrong scheme
		"example.com/api",         // no scheme
		"https://",                // no host
	}
	for _, bad := range badURLs {
		t.Run(bad, func(t *testing.T) {
			cfg := config.Config{
				Networks: map[string]config.NetworkConfig{
					"testnet4": {ProviderURLs: []string{bad}},
				},
			}
			path := writeConfig(t, cfg)

			_, err := config.Load(path)
			if err == nil {
				t.Errorf("Load with provider_url %q should have returned an error", bad)
			}
		})
	}
}

// Valid URLs (http allowed for local regtest).
func TestValidate_ValidURLsAccepted(t *testing.T) {
	goodURLs := []string{
		"https://mempool.space/testnet4/api",
		"https://blockstream.info/testnet/api",
		"http://localhost:3000",
		"http://127.0.0.1:8332",
	}
	for _, good := range goodURLs {
		t.Run(good, func(t *testing.T) {
			cfg := config.Config{
				Networks: map[string]config.NetworkConfig{
					"testnet4": {ProviderURLs: []string{good}},
				},
			}
			path := writeConfig(t, cfg)

			_, err := config.Load(path)
			if err != nil {
				t.Errorf("Load with provider_url %q returned unexpected error: %v", good, err)
			}
		})
	}
}

// Load must return a clear error for a nonexistent file.
func TestLoad_MissingFile(t *testing.T) {
	_, err := config.Load(filepath.Join(t.TempDir(), "does-not-exist.yaml"))
	if err == nil {
		t.Error("Load of nonexistent file should have returned an error")
	}
}

// Load must return a clear error for malformed YAML.
func TestLoad_MalformedYAML(t *testing.T) {
	f, err := os.CreateTemp(t.TempDir(), "*.yaml")
	if err != nil {
		t.Fatal(err)
	}
	f.WriteString(": not: valid: yaml: [[[")
	f.Close()

	_, err = config.Load(f.Name())
	if err == nil {
		t.Error("Load of malformed YAML should have returned an error")
	}
}

// NetworksTyped must return a map keyed by typed bitcoin.Network values.
func TestNetworksTyped(t *testing.T) {
	cfg := config.Config{
		Networks: map[string]config.NetworkConfig{
			"testnet4": {ProviderURLs: []string{"https://mempool.space/testnet4/api"}},
			"signet":   {ProviderURLs: []string{"https://mempool.space/signet/api"}},
		},
	}
	path := writeConfig(t, cfg)

	loaded, err := config.Load(path)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	typed := loaded.NetworksTyped()
	if len(typed) != 2 {
		t.Fatalf("NetworksTyped returned %d entries, want 2", len(typed))
	}
	for n, nc := range typed {
		if len(nc.ProviderURLs) == 0 {
			t.Errorf("network %q has empty ProviderURLs", n)
		}
	}
}
