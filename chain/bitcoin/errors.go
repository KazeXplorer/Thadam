package bitcoin

import (
	"errors"
	"fmt"
)

// ProviderUnavailableError is returned by a Provider when the upstream
// service is unreachable or responds with a server-side error (transport
// failure, HTTP 5xx). It signals that the operation may succeed if retried
// against a different provider.
//
// It is deliberately distinct from a plain error so that the failover layer
// can decide whether retrying is safe:
//
//   - ProviderUnavailableError on UTXOs / FeeEstimates → always retry next provider.
//   - ProviderUnavailableError on Broadcast → retry next provider (tx not submitted).
//   - Any other error on Broadcast → do NOT retry (tx may have been submitted
//     and rejected; retrying a rejected tx is wasted effort, and submitting
//     a valid tx twice is harmless but noisy).
type ProviderUnavailableError struct {
	// ProviderURL is the base URL of the provider that was unreachable.
	ProviderURL string
	// Cause is the underlying error (transport failure or non-OK HTTP status).
	Cause error
}

func (e *ProviderUnavailableError) Error() string {
	return fmt.Sprintf("provider %s unavailable: %v", e.ProviderURL, e.Cause)
}

func (e *ProviderUnavailableError) Unwrap() error { return e.Cause }

// IsProviderUnavailable reports whether err is (or wraps) a
// *ProviderUnavailableError, meaning a retry against a different provider
// is appropriate.
func IsProviderUnavailable(err error) bool {
	var t *ProviderUnavailableError
	return errors.As(err, &t)
}
