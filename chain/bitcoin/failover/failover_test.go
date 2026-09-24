package failover_test

import (
	"context"
	"errors"
	"testing"

	"thadam/chain/bitcoin"
	"thadam/chain/bitcoin/failover"
)

// --- Mock provider ---

// mockProvider is a controllable bitcoin.HealthAwareProvider for testing.
type mockProvider struct {
	url               string
	healthy           bool
	recovered         bool
	failureCount      int
	utxosResult       []bitcoin.UTXO
	utxosErr          error
	feeResult         bitcoin.FeeEstimates
	feeErr            error
	broadcastResult   bitcoin.BroadcastResult
	broadcastErr      error
	txsResult         []bitcoin.Transaction
	txsErr            error
	getTxResult       bitcoin.Transaction
	getTxErr          error
	blockHeightResult uint32
	blockHeightErr    error
	calls             map[string]int
}

func newMock() *mockProvider {
	return &mockProvider{
		url:     "https://mock.example.com",
		healthy: true,
		calls:   make(map[string]int),
	}
}

func (m *mockProvider) UTXOs(_ context.Context, _ string) ([]bitcoin.UTXO, error) {
	m.calls["UTXOs"]++
	return m.utxosResult, m.utxosErr
}

func (m *mockProvider) FeeEstimates(_ context.Context) (bitcoin.FeeEstimates, error) {
	m.calls["FeeEstimates"]++
	return m.feeResult, m.feeErr
}

func (m *mockProvider) Broadcast(_ context.Context, _ string) (bitcoin.BroadcastResult, error) {
	m.calls["Broadcast"]++
	return m.broadcastResult, m.broadcastErr
}

func (m *mockProvider) Transactions(_ context.Context, _ string) ([]bitcoin.Transaction, error) {
	m.calls["Transactions"]++
	return m.txsResult, m.txsErr
}

func (m *mockProvider) GetTx(_ context.Context, _ string) (bitcoin.Transaction, error) {
	m.calls["GetTx"]++
	return m.getTxResult, m.getTxErr
}

func (m *mockProvider) BlockHeight(_ context.Context) (uint32, error) {
	m.calls["BlockHeight"]++
	return m.blockHeightResult, m.blockHeightErr
}

// Health-aware methods.

func (m *mockProvider) IsHealthy() bool { return m.healthy }

func (m *mockProvider) RecordFailure() {
	m.failureCount++
	m.healthy = false
	m.calls["RecordFailure"]++
}

func (m *mockProvider) Recover(_ context.Context) {
	m.recovered = true
	m.healthy = true
	m.calls["Recover"]++
}

func (m *mockProvider) URL() string { return m.url }

func (m *mockProvider) HealthStatus() bitcoin.ProviderHealth {
	status := bitcoin.Healthy
	if !m.healthy {
		status = bitcoin.Unhealthy
	}
	return bitcoin.ProviderHealth{
		URL:      m.url,
		Status:   status,
		Failures: m.failureCount,
	}
}

// unavailErr creates a *bitcoin.ProviderUnavailableError to simulate a down provider.
func unavailErr(url string) error {
	return &bitcoin.ProviderUnavailableError{
		ProviderURL: url,
		Cause:       errors.New("connection refused"),
	}
}

// --- UTXOs tests ---

func TestUTXOs_FirstProviderSucceeds(t *testing.T) {
	want := []bitcoin.UTXO{{Txid: "abc", Value: 1000}}
	p1 := newMock()
	p1.utxosResult = want
	p2 := newMock()

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.UTXOs(context.Background(), "addr1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got) != 1 || got[0].Txid != "abc" {
		t.Errorf("UTXOs = %v, want %v", got, want)
	}
	if p2.calls["UTXOs"] != 0 {
		t.Error("second provider should not have been called")
	}
}

func TestUTXOs_FallsBackToSecondProvider(t *testing.T) {
	want := []bitcoin.UTXO{{Txid: "xyz", Value: 5000}}
	p1 := newMock()
	p1.utxosErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.utxosResult = want

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.UTXOs(context.Background(), "addr1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got) != 1 || got[0].Txid != "xyz" {
		t.Errorf("UTXOs = %v, want %v", got, want)
	}
	if p1.calls["UTXOs"] != 1 {
		t.Error("primary provider should have been called once")
	}
	if p2.calls["UTXOs"] != 1 {
		t.Error("fallback provider should have been called once")
	}
}

func TestUTXOs_AllProvidersUnavailable(t *testing.T) {
	p1 := newMock()
	p1.utxosErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.utxosErr = unavailErr("https://p2.example.com")

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	_, err := fp.UTXOs(context.Background(), "addr1")
	if err == nil {
		t.Fatal("expected error when all providers unavailable")
	}
}

// A non-ProviderUnavailableError must be returned immediately, without trying
// the next provider.
func TestUTXOs_DataErrorStopsRetry(t *testing.T) {
	dataErr := errors.New("malformed response")
	p1 := newMock()
	p1.utxosErr = dataErr
	p2 := newMock()
	p2.utxosResult = []bitcoin.UTXO{{Txid: "ok"}}

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	_, err := fp.UTXOs(context.Background(), "addr1")
	if !errors.Is(err, dataErr) {
		t.Errorf("expected dataErr, got %v", err)
	}
	if p2.calls["UTXOs"] != 0 {
		t.Error("second provider should not have been called after a data error")
	}
}

// RecordFailure must be called when a provider returns ProviderUnavailableError.
func TestUTXOs_RecordFailureCalledOnUnavailable(t *testing.T) {
	p1 := newMock()
	p1.utxosErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.utxosResult = []bitcoin.UTXO{{Txid: "ok"}}

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	_, err := fp.UTXOs(context.Background(), "addr1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if p1.calls["RecordFailure"] != 1 {
		t.Errorf("RecordFailure called %d times on unavailable provider, want 1", p1.calls["RecordFailure"])
	}
	if p2.calls["RecordFailure"] != 0 {
		t.Errorf("RecordFailure should not be called on the successful provider")
	}
}

// Unhealthy providers are tried last. If p1 is marked unhealthy before the
// request, p2 should be tried first.
func TestUTXOs_UnhealthyProviderDeprioritised(t *testing.T) {
	p1 := newMock()
	p1.healthy = false // already marked unhealthy
	p1.utxosResult = []bitcoin.UTXO{{Txid: "from-p1"}}
	p2 := newMock()
	p2.utxosResult = []bitcoin.UTXO{{Txid: "from-p2"}}

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.UTXOs(context.Background(), "addr1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// p2 is healthy — it should win even though p1 is listed first.
	if got[0].Txid != "from-p2" {
		t.Errorf("expected result from healthy p2, got %q", got[0].Txid)
	}
	if p1.calls["UTXOs"] != 0 {
		t.Errorf("unhealthy p1 should not have been called; called %d time(s)", p1.calls["UTXOs"])
	}
}

// --- FeeEstimates tests ---

func TestFeeEstimates_FallsBackOnUnavailable(t *testing.T) {
	p1 := newMock()
	p1.feeErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.feeResult = bitcoin.FeeEstimates{
		Fast:    bitcoin.FeeRate{SatPerVB: 10},
		Medium:  bitcoin.FeeRate{SatPerVB: 5},
		Economy: bitcoin.FeeRate{SatPerVB: 2},
	}

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.FeeEstimates(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Fast.SatPerVB != 10 {
		t.Errorf("Fast = %.1f, want 10", got.Fast.SatPerVB)
	}
}

// --- Broadcast tests ---

func TestBroadcast_FirstProviderSucceeds(t *testing.T) {
	p1 := newMock()
	p1.broadcastResult = bitcoin.BroadcastResult{Txid: "txid1"}
	p2 := newMock()

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.Broadcast(context.Background(), "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Txid != "txid1" {
		t.Errorf("Txid = %q, want txid1", got.Txid)
	}
	if p2.calls["Broadcast"] != 0 {
		t.Error("second provider should not have been called")
	}
}

func TestBroadcast_FallsBackOnUnavailable(t *testing.T) {
	p1 := newMock()
	p1.broadcastErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.broadcastResult = bitcoin.BroadcastResult{Txid: "txid2"}

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.Broadcast(context.Background(), "deadbeef")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Txid != "txid2" {
		t.Errorf("Txid = %q, want txid2", got.Txid)
	}
}

// A transaction rejected by the first provider must NOT be retried.
// Retrying a rejected tx is wrong: the rejection reason (invalid script,
// double-spend, policy violation) applies network-wide, not per-provider.
func TestBroadcast_TxRejectedDoesNotRetry(t *testing.T) {
	rejectionErr := errors.New("txn-mempool-conflict")
	p1 := newMock()
	p1.broadcastErr = rejectionErr // plain error, not ProviderUnavailableError
	p2 := newMock()
	p2.broadcastResult = bitcoin.BroadcastResult{Txid: "txid-fallback"}

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	_, err := fp.Broadcast(context.Background(), "deadbeef")
	if !errors.Is(err, rejectionErr) {
		t.Errorf("expected rejectionErr, got %v", err)
	}
	if p2.calls["Broadcast"] != 0 {
		t.Errorf("second provider should never be tried after a tx rejection; called %d time(s)", p2.calls["Broadcast"])
	}
}

// All providers unavailable on Broadcast — all tried, last error returned.
func TestBroadcast_AllProvidersUnavailable(t *testing.T) {
	p1 := newMock()
	p1.broadcastErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.broadcastErr = unavailErr("https://p2.example.com")

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	_, err := fp.Broadcast(context.Background(), "deadbeef")
	if err == nil {
		t.Fatal("expected error when all providers unavailable")
	}
	if p1.calls["Broadcast"] != 1 {
		t.Errorf("p1.Broadcast called %d times, want 1", p1.calls["Broadcast"])
	}
	if p2.calls["Broadcast"] != 1 {
		t.Errorf("p2.Broadcast called %d times, want 1", p2.calls["Broadcast"])
	}
}

// New panics if called with an empty slice — it would silently never make any
// real call, which is worse than a clear startup failure.
func TestNew_PanicsOnEmpty(t *testing.T) {
	defer func() {
		if r := recover(); r == nil {
			t.Error("failover.New([]) should have panicked")
		}
	}()
	failover.New(nil)
}

// --- Transactions tests ---

func TestTransactions_FallsBackOnUnavailable(t *testing.T) {
	want := []bitcoin.Transaction{{Txid: "abc", Fee: 500}}
	p1 := newMock()
	p1.txsErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.txsResult = want

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.Transactions(context.Background(), "addr1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got) != 1 || got[0].Txid != "abc" {
		t.Errorf("Transactions = %v, want %v", got, want)
	}
	if p1.calls["RecordFailure"] != 1 {
		t.Errorf("RecordFailure called %d times on unavailable provider, want 1", p1.calls["RecordFailure"])
	}
}

// --- GetTx tests ---

func TestGetTx_FallsBackOnUnavailable(t *testing.T) {
	want := bitcoin.Transaction{Txid: "txabc", Fee: 1000}
	p1 := newMock()
	p1.getTxErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.getTxResult = want

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.GetTx(context.Background(), "txabc")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got.Txid != "txabc" {
		t.Errorf("GetTx.Txid = %q, want txabc", got.Txid)
	}
}

// --- BlockHeight tests ---

func TestBlockHeight_FallsBackOnUnavailable(t *testing.T) {
	p1 := newMock()
	p1.blockHeightErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.blockHeightResult = 850000

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	got, err := fp.BlockHeight(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got != 850000 {
		t.Errorf("BlockHeight = %d, want 850000", got)
	}
}

func TestBlockHeight_AllUnavailable(t *testing.T) {
	p1 := newMock()
	p1.blockHeightErr = unavailErr("https://p1.example.com")
	p2 := newMock()
	p2.blockHeightErr = unavailErr("https://p2.example.com")

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	_, err := fp.BlockHeight(context.Background())
	if err == nil {
		t.Fatal("expected error when all providers unavailable")
	}
}

// --- NetworkHealth tests ---

func TestNetworkHealth_Reports(t *testing.T) {
	p1 := newMock()
	p1.url = "https://primary.example.com"
	p2 := newMock()
	p2.url = "https://fallback.example.com"
	p2.healthy = false
	p2.failureCount = 2

	fp := failover.New([]bitcoin.HealthAwareProvider{p1, p2})
	health := fp.NetworkHealth()

	if len(health) != 2 {
		t.Fatalf("NetworkHealth returned %d entries, want 2", len(health))
	}
	// Declaration order is preserved in the snapshot.
	if health[0].URL != "https://primary.example.com" {
		t.Errorf("health[0].URL = %q, want primary", health[0].URL)
	}
	if health[0].Status != bitcoin.Healthy {
		t.Errorf("health[0].Status = %q, want healthy", health[0].Status)
	}
	if health[1].URL != "https://fallback.example.com" {
		t.Errorf("health[1].URL = %q, want fallback", health[1].URL)
	}
	if health[1].Status != bitcoin.Unhealthy {
		t.Errorf("health[1].Status = %q, want unhealthy", health[1].Status)
	}
	if health[1].Failures != 2 {
		t.Errorf("health[1].Failures = %d, want 2", health[1].Failures)
	}
}
