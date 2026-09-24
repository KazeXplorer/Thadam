package bitcoin

import "context"

// ProviderStatus is the health state of a single provider endpoint.
// Mirrors the ChainOps pattern (providers/provider.go).
type ProviderStatus string

const (
	// Healthy means the provider responded successfully on the most recent probe.
	Healthy ProviderStatus = "healthy"
	// Unhealthy means the provider has failed at least once and has not yet
	// recovered.
	Unhealthy ProviderStatus = "unhealthy"
)

// ProviderHealth is a point-in-time snapshot of one provider URL.
// Used in the /health response and for logging.
type ProviderHealth struct {
	URL      string         `json:"url"`
	Status   ProviderStatus `json:"status"`
	Failures int            `json:"failures"`
}

// HealthAwareProvider extends Provider with the health-tracking contract from
// ChainOps's providers/provider.go, adapted for Esplora REST endpoints.
//
// The failover layer uses this interface to:
//   - Skip known-unhealthy providers on incoming requests (try healthy first).
//   - Call RecordFailure when a provider returns ProviderUnavailableError.
//   - Call Recover periodically in a background goroutine.
type HealthAwareProvider interface {
	Provider

	// IsHealthy reports whether this provider is considered reachable.
	// Returns false after RecordFailure is called, until Recover succeeds.
	IsHealthy() bool

	// RecordFailure marks a failed attempt. Transitions the provider to
	// Unhealthy so subsequent requests skip it until recovery.
	RecordFailure()

	// Recover probes the live endpoint. On success it resets the failure
	// count and transitions back to Healthy. Called periodically by the
	// failover background goroutine — not on the hot request path.
	Recover(ctx context.Context)

	// URL returns the provider's base URL, used for logging and health reports.
	URL() string

	// HealthStatus returns a snapshot of this provider's current health.
	HealthStatus() ProviderHealth
}

// HealthReporter is implemented by types that aggregate health across multiple
// providers (the failover.Provider). The server uses it to build the /health
// JSON response.
type HealthReporter interface {
	NetworkHealth() []ProviderHealth
}
