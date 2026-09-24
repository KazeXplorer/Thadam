// Package failover wraps a prioritised list of bitcoin.HealthAwareProviders
// and routes each operation to healthy providers first, falling back to
// unhealthy ones only when all healthy options are exhausted.
//
// Design rules (see ARCHITECTURE.md §8):
//
//  1. UTXOs and FeeEstimates: retry across every provider (healthy first) until
//     one succeeds. A ProviderUnavailableError means "try the next one".
//     Any other error is returned immediately — it indicates a data problem,
//     not a connectivity problem.
//
//  2. Broadcast: same retry behaviour, but on a non-ProviderUnavailableError
//     (transaction rejected by the mempool), stop immediately. A rejected
//     transaction is rejected for a protocol reason (invalid script, dust,
//     double-spend) that will not change between providers.
//
//  3. Health tracking: RecordFailure is called on every ProviderUnavailableError
//     so the provider transitions to Unhealthy and is deprioritised on subsequent
//     requests. Start() runs a background goroutine that calls Recover() on
//     unhealthy providers every 30 seconds.
//
// If all providers fail with ProviderUnavailableError the last error is
// returned, wrapped to indicate that all providers were tried.
package failover

import (
	"context"
	"fmt"
	"sync"
	"time"

	"thadam/chain/bitcoin"
)

// Provider wraps a slice of bitcoin.HealthAwareProviders and tries them in
// order with healthy providers first.
//
// Create one with New. The first provider in the slice is the primary; later
// entries are fallbacks. All providers must serve the same network.
type Provider struct {
	mu        sync.RWMutex
	providers []bitcoin.HealthAwareProvider
}

// New creates a failover Provider. At least one provider is required.
func New(providers []bitcoin.HealthAwareProvider) *Provider {
	if len(providers) == 0 {
		panic("failover.New: at least one provider is required")
	}
	return &Provider{providers: providers}
}

// Compile-time checks.
var _ bitcoin.Provider = (*Provider)(nil)
var _ bitcoin.HealthReporter = (*Provider)(nil)

// Start launches a background goroutine that calls Recover on unhealthy
// providers every 30 seconds. It returns immediately. The goroutine runs until
// ctx is cancelled — pass a context tied to the process lifetime.
//
// This is the recovery half of the health-tracking loop: RecordFailure marks a
// provider unhealthy on the hot path; Recover brings it back once it is
// reachable again.
func (p *Provider) Start(ctx context.Context) {
	go func() {
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				p.mu.RLock()
				provs := make([]bitcoin.HealthAwareProvider, len(p.providers))
				copy(provs, p.providers)
				p.mu.RUnlock()
				for _, prov := range provs {
					if !prov.IsHealthy() {
						prov.Recover(ctx)
					}
				}
			}
		}
	}()
}

// NetworkHealth implements bitcoin.HealthReporter. It returns a snapshot of
// every underlying provider's health, in declaration order.
func (p *Provider) NetworkHealth() []bitcoin.ProviderHealth {
	p.mu.RLock()
	defer p.mu.RUnlock()
	out := make([]bitcoin.ProviderHealth, len(p.providers))
	for i, prov := range p.providers {
		out[i] = prov.HealthStatus()
	}
	return out
}

// orderedProviders returns healthy providers first, unhealthy ones last.
// This is the core of health-aware routing: known-bad providers are skipped on
// the hot path but still tried as a last resort if all healthy ones fail.
func (p *Provider) orderedProviders() []bitcoin.HealthAwareProvider {
	p.mu.RLock()
	defer p.mu.RUnlock()
	healthy := make([]bitcoin.HealthAwareProvider, 0, len(p.providers))
	unhealthy := make([]bitcoin.HealthAwareProvider, 0)
	for _, prov := range p.providers {
		if prov.IsHealthy() {
			healthy = append(healthy, prov)
		} else {
			unhealthy = append(unhealthy, prov)
		}
	}
	return append(healthy, unhealthy...)
}

// tryProviders runs fn against each provider in health-priority order.
//
//   - On success (fn returns nil) it returns immediately.
//   - On ProviderUnavailableError it calls RecordFailure on that provider and
//     tries the next one.
//   - On any other error it returns immediately without trying further providers
//     (data/protocol error — retrying cannot help).
//
// If all providers return ProviderUnavailableError, the last such error is
// returned.
func (p *Provider) tryProviders(fn func(bitcoin.HealthAwareProvider) error) error {
	ordered := p.orderedProviders()
	var lastErr error
	for _, prov := range ordered {
		err := fn(prov)
		if err == nil {
			return nil
		}
		if !bitcoin.IsProviderUnavailable(err) {
			// Protocol/data error — stop immediately.
			return err
		}
		// Provider is down; mark it and try the next.
		prov.RecordFailure()
		lastErr = err
	}
	return fmt.Errorf("all providers unavailable: %w", lastErr)
}

// UTXOs implements bitcoin.Provider.
func (p *Provider) UTXOs(ctx context.Context, address string) ([]bitcoin.UTXO, error) {
	var result []bitcoin.UTXO
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.UTXOs(ctx, address)
		return e
	})
	return result, err
}

// FeeEstimates implements bitcoin.Provider.
func (p *Provider) FeeEstimates(ctx context.Context) (bitcoin.FeeEstimates, error) {
	var result bitcoin.FeeEstimates
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.FeeEstimates(ctx)
		return e
	})
	return result, err
}

// Broadcast implements bitcoin.Provider.
//
// Only retries on ProviderUnavailableError. A transaction rejection (any other
// error) is returned immediately — the rejection reason applies network-wide.
func (p *Provider) Broadcast(ctx context.Context, rawTx string) (bitcoin.BroadcastResult, error) {
	var result bitcoin.BroadcastResult
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.Broadcast(ctx, rawTx)
		return e
	})
	return result, err
}

// Transactions implements bitcoin.Provider.
func (p *Provider) Transactions(ctx context.Context, address string) ([]bitcoin.Transaction, error) {
	var result []bitcoin.Transaction
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.Transactions(ctx, address)
		return e
	})
	return result, err
}

// GetTx implements bitcoin.Provider.
func (p *Provider) GetTx(ctx context.Context, txid string) (bitcoin.Transaction, error) {
	var result bitcoin.Transaction
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.GetTx(ctx, txid)
		return e
	})
	return result, err
}

// BlockHeight implements bitcoin.Provider.
func (p *Provider) BlockHeight(ctx context.Context) (uint32, error) {
	var result uint32
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.BlockHeight(ctx)
		return e
	})
	return result, err
}

// TransactionsSince implements bitcoin.Provider.
func (p *Provider) TransactionsSince(ctx context.Context, address, lastSeenTxid string) ([]bitcoin.Transaction, error) {
	var result []bitcoin.Transaction
	err := p.tryProviders(func(prov bitcoin.HealthAwareProvider) error {
		var e error
		result, e = prov.TransactionsSince(ctx, address, lastSeenTxid)
		return e
	})
	return result, err
}
