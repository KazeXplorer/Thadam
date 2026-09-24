// Package esplora implements the bitcoin.Provider interface against the
// Esplora REST API.
//
// Esplora is the API shape used by both mempool.space and Blockstream's
// public explorer. The documented API is at:
//   https://github.com/Blockstream/esplora/blob/master/API.md
//
// The specific instance URL is runtime config (config.Config). It is not
// hardcoded here, because ARCHITECTURE.md §10 blocks any URL that has not
// been verified against a live service. Known working values for Phase 1:
//
//   testnet4: https://mempool.space/testnet4/api
//   signet:   https://mempool.space/signet/api
//   regtest:  http://localhost:3000             (local Esplora instance)
//
// These are documented, public APIs; the testnet4 and signet values are not
// compiled in because endpoint URLs are runtime config per A.1.
package esplora

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strings"
	"sync"
	"time"

	"thadam/chain/bitcoin"
)

// Client is an Esplora HTTP client. It implements bitcoin.HealthAwareProvider.
//
// Create one with New; the zero value is not usable.
type Client struct {
	// baseURL is the root of the Esplora API endpoint, without a trailing
	// slash. Supplied by the caller, validated by config before reaching here.
	baseURL string

	// http is the underlying HTTP client. A custom client with a timeout is
	// used so that a slow or unresponsive provider cannot stall the service
	// indefinitely.
	http *http.Client

	// log is the structured logger for this client instance.
	log *slog.Logger

	// mu guards failureCount and status. RecordFailure and Recover may be
	// called concurrently from multiple goroutines.
	mu           sync.Mutex
	failureCount int
	status       bitcoin.ProviderStatus
}

// New creates a Client for the given Esplora base URL.
//
// The URL must not have a trailing slash. It is the caller's responsibility
// to supply a URL that has been verified against a live service (§10).
func New(baseURL string, log *slog.Logger) *Client {
	return &Client{
		baseURL: strings.TrimRight(baseURL, "/"),
		http: &http.Client{
			Timeout: 30 * time.Second,
		},
		log:    log.With("provider", "esplora", "base_url", baseURL),
		status: bitcoin.Healthy,
	}
}

// Compile-time check that Client satisfies bitcoin.HealthAwareProvider.
var _ bitcoin.HealthAwareProvider = (*Client)(nil)

// --- Health tracking (mirrors ChainOps providers/evm_provider.go) ---

// IsHealthy reports whether this provider is considered reachable.
func (c *Client) IsHealthy() bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.status != bitcoin.Unhealthy
}

// RecordFailure increments the failure count and marks the provider unhealthy.
// Any single failure is enough to trigger failover — fast failover beats slow
// retries on the hot request path.
func (c *Client) RecordFailure() {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.failureCount++
	c.status = bitcoin.Unhealthy
	c.log.Warn("provider marked unhealthy",
		"url", c.baseURL,
		"failureCount", c.failureCount,
	)
}

// Recover probes the provider with a lightweight request (GET /fee-estimates).
// On success it resets the failure count and transitions back to Healthy.
// Called periodically by the failover background goroutine — not on the hot
// request path.
func (c *Client) Recover(ctx context.Context) {
	probeCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()

	url := c.baseURL + "/fee-estimates"
	_, err := c.get(probeCtx, url)
	if err != nil {
		c.log.Warn("provider still unhealthy", "url", c.baseURL, "error", err)
		return
	}

	c.mu.Lock()
	c.failureCount = 0
	c.status = bitcoin.Healthy
	c.mu.Unlock()
	c.log.Info("provider recovered", "url", c.baseURL)
}

// URL returns the provider's base URL.
func (c *Client) URL() string { return c.baseURL }

// HealthStatus returns a point-in-time snapshot of this provider's health.
func (c *Client) HealthStatus() bitcoin.ProviderHealth {
	c.mu.Lock()
	defer c.mu.Unlock()
	return bitcoin.ProviderHealth{
		URL:      c.baseURL,
		Status:   c.status,
		Failures: c.failureCount,
	}
}

// UTXOs implements bitcoin.Provider.
//
// Maps to GET {base}/address/{address}/utxo.
//
// Esplora returns both confirmed and unconfirmed UTXOs in one response. The
// response is a JSON array; each element has the shape:
//
//	{
//	  "txid": "abc123...",
//	  "vout": 0,
//	  "status": { "confirmed": true, "block_height": 12345 },
//	  "value": 100000
//	}
func (c *Client) UTXOs(ctx context.Context, address string) ([]bitcoin.UTXO, error) {
	url := fmt.Sprintf("%s/address/%s/utxo", c.baseURL, address)
	body, err := c.get(ctx, url)
	if err != nil {
		return nil, fmt.Errorf("esplora UTXOs: %w", err)
	}

	// Inline type matching the Esplora wire format exactly.
	var raw []struct {
		Txid   string `json:"txid"`
		Vout   uint32 `json:"vout"`
		Value  uint64 `json:"value"`
		Status struct {
			Confirmed   bool   `json:"confirmed"`
			BlockHeight uint32 `json:"block_height"`
		} `json:"status"`
	}
	if err := json.Unmarshal(body, &raw); err != nil {
		return nil, fmt.Errorf("esplora UTXOs: malformed response: %w", err)
	}

	utxos := make([]bitcoin.UTXO, len(raw))
	for i, r := range raw {
		utxos[i] = bitcoin.UTXO{
			Txid:        r.Txid,
			Vout:        r.Vout,
			Value:       r.Value,
			Confirmed:   r.Status.Confirmed,
			BlockHeight: r.Status.BlockHeight,
		}
	}
	return utxos, nil
}

// FeeEstimates implements bitcoin.Provider.
//
// Maps to GET {base}/fee-estimates.
//
// Esplora returns a JSON object mapping confirmation target (number of blocks,
// as a string key) to fee rate in sat/vB:
//
//	{ "1": 25.3, "3": 18.1, "6": 12.0, ... }
//
// The A.14 sanity bounds are applied to every value. If a rate exceeds
// MaxSatPerVB it is clamped to that value and the event is logged — the client
// still gets a usable response, and the log captures the anomaly for
// investigation.
func (c *Client) FeeEstimates(ctx context.Context) (bitcoin.FeeEstimates, error) {
	url := fmt.Sprintf("%s/fee-estimates", c.baseURL)
	body, err := c.get(ctx, url)
	if err != nil {
		return bitcoin.FeeEstimates{}, fmt.Errorf("esplora FeeEstimates: %w", err)
	}

	// Esplora keys are confirmation targets (strings like "1", "3", "6").
	var raw map[string]float64
	if err := json.Unmarshal(body, &raw); err != nil {
		return bitcoin.FeeEstimates{}, fmt.Errorf("esplora FeeEstimates: malformed response: %w", err)
	}

	fast := c.pickRate(raw, "1", "2")
	medium := c.pickRate(raw, "3", "4", "5", "6")
	economy := c.pickRate(raw, "10", "12", "25", "144")

	if fast == 0 && medium == 0 && economy == 0 {
		return bitcoin.FeeEstimates{}, fmt.Errorf("esplora FeeEstimates: response contained no usable confirmation targets")
	}

	// Fall back to faster tier if slower tier is missing.
	if medium == 0 {
		medium = fast
	}
	if economy == 0 {
		economy = medium
	}
	if fast == 0 {
		fast = medium
	}

	return bitcoin.FeeEstimates{
		Fast:    c.validateAndLog("fast", fast),
		Medium:  c.validateAndLog("medium", medium),
		Economy: c.validateAndLog("economy", economy),
	}, nil
}

// Broadcast implements bitcoin.Provider.
//
// Maps to POST {base}/tx with the raw transaction as a hex-encoded string in
// the request body (plain text, not JSON).
//
// Esplora returns the txid as a plain hex string on success, or an error
// message with an appropriate HTTP status code on failure.
func (c *Client) Broadcast(ctx context.Context, rawTx string) (bitcoin.BroadcastResult, error) {
	url := fmt.Sprintf("%s/tx", c.baseURL)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, strings.NewReader(rawTx))
	if err != nil {
		return bitcoin.BroadcastResult{}, fmt.Errorf("esplora Broadcast: build request: %w", err)
	}
	req.Header.Set("Content-Type", "text/plain")

	resp, err := c.http.Do(req)
	if err != nil {
		// Transport failure before the tx reached the provider.
		// Safe to retry against a different provider.
		return bitcoin.BroadcastResult{}, &bitcoin.ProviderUnavailableError{
			ProviderURL: c.baseURL,
			Cause:       fmt.Errorf("esplora Broadcast: %w", err),
		}
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(io.LimitReader(resp.Body, 4096))
	if err != nil {
		return bitcoin.BroadcastResult{}, fmt.Errorf("esplora Broadcast: read response: %w", err)
	}

	if resp.StatusCode >= 500 {
		// Server error: provider accepted the request but failed internally.
		// The tx likely did not reach the mempool — safe to retry.
		return bitcoin.BroadcastResult{}, &bitcoin.ProviderUnavailableError{
			ProviderURL: c.baseURL,
			Cause: fmt.Errorf(
				"esplora Broadcast: server error (HTTP %d): %s",
				resp.StatusCode, strings.TrimSpace(string(body)),
			),
		}
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		// 4xx: provider understood the request but rejected the transaction.
		// Do NOT retry — the rejection reason applies regardless of which
		// provider sees it (invalid script, double-spend, etc.).
		return bitcoin.BroadcastResult{}, fmt.Errorf(
			"esplora Broadcast: transaction rejected (HTTP %d): %s",
			resp.StatusCode, strings.TrimSpace(string(body)),
		)
	}

	txid := strings.TrimSpace(string(body))
	c.log.InfoContext(ctx, "transaction broadcast accepted", "txid", txid)
	return bitcoin.BroadcastResult{Txid: txid}, nil
}

// esploraRawTx is the Esplora wire format for a transaction, shared across
// Transactions, TransactionsSince, and GetTx.
//
//	{
//	  "txid": "abc123...",
//	  "fee": 1500,
//	  "status": { "confirmed": true, "block_height": 800000, "block_time": 1700000000 },
//	  "vin": [
//	    {
//	      "txid": "prev_txid...",
//	      "vout": 0,
//	      "is_coinbase": false,
//	      "prevout": { "scriptpubkey_address": "bc1q...", "value": 100000 }
//	    }
//	  ],
//	  "vout": [
//	    { "scriptpubkey_address": "bc1q...", "value": 50000 },
//	    ...
//	  ]
//	}
type esploraRawTx struct {
	Txid   string `json:"txid"`
	Fee    uint64 `json:"fee"`
	Status struct {
		Confirmed   bool   `json:"confirmed"`
		BlockHeight uint32 `json:"block_height"`
		BlockTime   int64  `json:"block_time"`
	} `json:"status"`
	Vin []struct {
		Txid       string `json:"txid"`
		Vout       uint32 `json:"vout"`
		IsCoinbase bool   `json:"is_coinbase"`
		Prevout    struct {
			Address string `json:"scriptpubkey_address"`
		} `json:"prevout"`
	} `json:"vin"`
	Vout []struct {
		Value   uint64 `json:"value"`
		Address string `json:"scriptpubkey_address"`
	} `json:"vout"`
}

// toTransaction converts an Esplora wire transaction to a bitcoin.Transaction.
func toTransaction(r esploraRawTx) bitcoin.Transaction {
	vin := make([]bitcoin.TxInput, len(r.Vin))
	for i, in := range r.Vin {
		vin[i] = bitcoin.TxInput{
			PrevTxid: in.Txid,
			PrevVout: in.Vout,
			Address:  in.Prevout.Address, // empty for coinbase
		}
	}
	vout := make([]bitcoin.TxOutput, len(r.Vout))
	for i, o := range r.Vout {
		vout[i] = bitcoin.TxOutput{
			Index:   uint32(i),
			Value:   o.Value,
			Address: o.Address,
		}
	}
	return bitcoin.Transaction{
		Txid: r.Txid,
		Fee:  r.Fee,
		Status: bitcoin.TxStatus{
			Confirmed:   r.Status.Confirmed,
			BlockHeight: r.Status.BlockHeight,
			BlockTime:   r.Status.BlockTime,
		},
		Vin:  vin,
		Vout: vout,
	}
}

// Transactions implements bitcoin.Provider.
//
// Maps to GET {base}/address/{address}/txs.
// Returns up to 25 transactions (confirmed + mempool), newest first.
func (c *Client) Transactions(ctx context.Context, address string) ([]bitcoin.Transaction, error) {
	url := fmt.Sprintf("%s/address/%s/txs", c.baseURL, address)
	body, err := c.get(ctx, url)
	if err != nil {
		return nil, fmt.Errorf("esplora Transactions: %w", err)
	}

	var raw []esploraRawTx
	if err := json.Unmarshal(body, &raw); err != nil {
		return nil, fmt.Errorf("esplora Transactions: malformed response: %w", err)
	}

	txs := make([]bitcoin.Transaction, len(raw))
	for i, r := range raw {
		txs[i] = toTransaction(r)
	}
	return txs, nil
}

// TransactionsSince implements bitcoin.Provider.
//
// Maps to GET {base}/address/{address}/txs/chain/{last_seen_txid}.
//
// Returns up to 25 confirmed transactions older than lastSeenTxid, newest
// first. This is the pagination endpoint for incremental sync: the mobile app
// passes the oldest txid it has stored locally to page back through history
// without re-fetching what it already holds.
func (c *Client) TransactionsSince(ctx context.Context, address, lastSeenTxid string) ([]bitcoin.Transaction, error) {
	url := fmt.Sprintf("%s/address/%s/txs/chain/%s", c.baseURL, address, lastSeenTxid)
	body, err := c.get(ctx, url)
	if err != nil {
		return nil, fmt.Errorf("esplora TransactionsSince: %w", err)
	}

	var raw []esploraRawTx
	if err := json.Unmarshal(body, &raw); err != nil {
		return nil, fmt.Errorf("esplora TransactionsSince: malformed response: %w", err)
	}

	txs := make([]bitcoin.Transaction, len(raw))
	for i, r := range raw {
		txs[i] = toTransaction(r)
	}
	return txs, nil
}

// GetTx implements bitcoin.Provider.
//
// Maps to GET {base}/tx/{txid}.
func (c *Client) GetTx(ctx context.Context, txid string) (bitcoin.Transaction, error) {
	url := fmt.Sprintf("%s/tx/%s", c.baseURL, txid)
	body, err := c.get(ctx, url)
	if err != nil {
		return bitcoin.Transaction{}, fmt.Errorf("esplora GetTx: %w", err)
	}

	var raw esploraRawTx
	if err := json.Unmarshal(body, &raw); err != nil {
		return bitcoin.Transaction{}, fmt.Errorf("esplora GetTx: malformed response: %w", err)
	}

	return toTransaction(raw), nil
}

// BlockHeight implements bitcoin.Provider.
//
// Maps to GET {base}/blocks/tip/height.
//
// Esplora returns the current best block height as a plain integer (not JSON).
func (c *Client) BlockHeight(ctx context.Context) (uint32, error) {
	url := fmt.Sprintf("%s/blocks/tip/height", c.baseURL)
	body, err := c.get(ctx, url)
	if err != nil {
		return 0, fmt.Errorf("esplora BlockHeight: %w", err)
	}

	var height uint32
	if err := json.Unmarshal(body, &height); err != nil {
		return 0, fmt.Errorf("esplora BlockHeight: malformed response %q: %w", bytes.TrimSpace(body), err)
	}
	return height, nil
}

// get performs a GET request and returns the response body.
//
// Transport failures and HTTP 5xx responses are wrapped in a
// *bitcoin.ProviderUnavailableError so that the failover layer can
// distinguish "provider down" from "provider up but returned bad data".
func (c *Client) get(ctx context.Context, url string) ([]byte, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, fmt.Errorf("build request: %w", err)
	}

	resp, err := c.http.Do(req)
	if err != nil {
		// Transport failure: provider is unreachable — safe to retry elsewhere.
		return nil, &bitcoin.ProviderUnavailableError{
			ProviderURL: c.baseURL,
			Cause:       fmt.Errorf("HTTP GET %s: %w", url, err),
		}
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 500 {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		// Server-side error: provider is up but broken — safe to retry elsewhere.
		return nil, &bitcoin.ProviderUnavailableError{
			ProviderURL: c.baseURL,
			Cause:       fmt.Errorf("HTTP GET %s: status %d: %s", url, resp.StatusCode, bytes.TrimSpace(body)),
		}
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		return nil, fmt.Errorf("HTTP GET %s: status %d: %s", url, resp.StatusCode, bytes.TrimSpace(body))
	}

	body, err := io.ReadAll(io.LimitReader(resp.Body, 1<<20)) // 1 MiB limit
	if err != nil {
		return nil, fmt.Errorf("HTTP GET %s: read response: %w", url, err)
	}
	return body, nil
}

// pickRate returns the first non-zero rate found for any of the given
// confirmation targets, in preference order.
func (c *Client) pickRate(m map[string]float64, targets ...string) float64 {
	for _, t := range targets {
		if r, ok := m[t]; ok && r > 0 {
			return r
		}
	}
	return 0
}

// validateAndLog applies the A.14 sanity bounds to a rate.
//
// If the rate is above MaxSatPerVB it is clamped to that value and the event
// is logged. If the rate is below MinSatPerVB it is raised to that floor.
// Both anomalies are logged; neither causes the method to return an error,
// because a clamped but usable response is better than no response at all.
// A rate that triggers the clamp is almost certainly a provider bug; the log
// is the signal for investigation.
func (c *Client) validateAndLog(tier string, satPerVB float64) bitcoin.FeeRate {
	clamped := satPerVB
	if clamped < bitcoin.MinSatPerVB {
		c.log.Warn("fee rate below minimum, clamping",
			"tier", tier, "provider_rate", satPerVB, "floor", bitcoin.MinSatPerVB)
		clamped = bitcoin.MinSatPerVB
	}
	if clamped > bitcoin.MaxSatPerVB {
		c.log.Warn("fee rate above plausible maximum, clamping",
			"tier", tier, "provider_rate", satPerVB, "ceiling", bitcoin.MaxSatPerVB)
		clamped = bitcoin.MaxSatPerVB
	}

	f, err := bitcoin.ValidateFeeRate(clamped)
	if err != nil {
		// ValidateFeeRate can only error if the value is outside [Min, Max].
		// We just clamped to that range, so this cannot happen unless there is
		// a bug in the clamping logic above.
		panic(fmt.Sprintf("esplora: ValidateFeeRate on clamped value: %v", err))
	}
	return f
}
