// Package config loads and validates the chain-service configuration from a
// YAML file.
//
// ARCHITECTURE.md A.1: "The registry holds constants; endpoints are runtime
// config." Consensus constants are compiled into bitcoin/network.go; this
// package holds the configuration that changes per deploy — provider URLs.
//
// At startup the service loads a YAML config file and calls Config.Validate.
// Validation rejects any network key that is not in the compiled catalogue
// (bitcoin.ParseNetwork). A config that names an unknown network is an error
// at load time, not a silent no-op at query time.
//
// Example config.yaml:
//
//	listen_addr: ":8080"
//	networks:
//	  testnet4:
//	    provider_urls:
//	      - "https://mempool.space/testnet4/api"
//	      - "https://blockstream.info/testnet/api"
//	  signet:
//	    provider_urls:
//	      - "https://mempool.space/signet/api"
package config

import (
	"fmt"
	"net/url"
	"os"
	"strings"

	"gopkg.in/yaml.v3"

	"thadam/chain/bitcoin"
)

// NetworkConfig holds the provider configuration for one Bitcoin network.
// ProviderURLs lists one or more Esplora-compatible base URLs tried in order;
// the bitcoin/failover package wraps them into a single Provider.
type NetworkConfig struct {
	// ProviderURLs is an ordered list of Esplora API base URLs for this
	// network. The first reachable URL is tried first; subsequent entries
	// are used when the earlier ones are unavailable.
	//
	// At least one URL is required. Known working values (verify before use):
	//   testnet4: https://mempool.space/testnet4/api
	//             https://blockstream.info/testnet/api
	//   signet:   https://mempool.space/signet/api
	//   regtest:  http://localhost:3000
	ProviderURLs []string `yaml:"provider_urls"`
}

// Config is the full runtime configuration for the chain service.
type Config struct {
	// ListenAddr is the address the HTTP server binds to (e.g. ":8080").
	// Defaults to ":8080" if empty.
	ListenAddr string `yaml:"listen_addr"`

	// Networks maps a network identifier (e.g. "testnet4") to its runtime
	// configuration. Only networks listed here are served.
	Networks map[string]NetworkConfig `yaml:"networks"`
}

// Load reads, parses, and validates the YAML file at path, returning a fully
// validated Config or a descriptive error.
func Load(path string) (Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return Config{}, fmt.Errorf("open config %q: %w", path, err)
	}

	var cfg Config
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return Config{}, fmt.Errorf("parse config %q: %w", path, err)
	}

	if err := cfg.Validate(); err != nil {
		return Config{}, fmt.Errorf("invalid config %q: %w", path, err)
	}
	return cfg, nil
}

// Validate returns an error if the config is invalid.
//
// Validation rules:
//   - At least one network must be configured.
//   - Every key in Networks must be in the compiled catalogue.
//   - Every NetworkConfig must have at least one ProviderURL.
//   - Every ProviderURL must be a parseable HTTP(S) URL with a non-empty host.
func (c *Config) Validate() error {
	if len(c.Networks) == 0 {
		return fmt.Errorf("no networks configured: at least one entry under \"networks\" is required")
	}

	for key, nc := range c.Networks {
		if _, err := bitcoin.ParseNetwork(key); err != nil {
			return fmt.Errorf("networks.%q: %w", key, err)
		}
		if len(nc.ProviderURLs) == 0 {
			return fmt.Errorf("networks.%q.provider_urls: at least one URL required", key)
		}
		for i, u := range nc.ProviderURLs {
			if err := validateURL(u); err != nil {
				return fmt.Errorf("networks.%q.provider_urls[%d]: %w", key, i, err)
			}
		}
	}

	if c.ListenAddr == "" {
		c.ListenAddr = ":8080"
	}

	return nil
}

// NetworksTyped returns the configured networks as typed bitcoin.Network values,
// paired with their NetworkConfig. The outer validate step guarantees every key
// is parseable.
func (c *Config) NetworksTyped() map[bitcoin.Network]NetworkConfig {
	m := make(map[bitcoin.Network]NetworkConfig, len(c.Networks))
	for k, v := range c.Networks {
		n, _ := bitcoin.ParseNetwork(k) // safe: Validate already checked
		m[n] = v
	}
	return m
}

// validateURL checks that s is a non-empty, parseable URL with an http or https
// scheme and a non-empty host. It does not make a network request.
func validateURL(s string) error {
	if strings.TrimSpace(s) == "" {
		return fmt.Errorf("empty URL")
	}
	u, err := url.ParseRequestURI(s)
	if err != nil {
		return fmt.Errorf("not a valid URL: %w", err)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return fmt.Errorf("scheme %q is not http or https", u.Scheme)
	}
	if u.Host == "" {
		return fmt.Errorf("URL has no host")
	}
	return nil
}
