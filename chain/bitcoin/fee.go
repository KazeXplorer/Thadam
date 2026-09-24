package bitcoin

import "fmt"

// Fee rate constants. These are hardcoded in the binary.
//
// ARCHITECTURE.md A.14: "The fee rate is the only path from a lying chain
// service to lost user funds. A hardcoded plausible range plus explicit
// confirmation for anything unusual closes it for a few lines."
//
// The only thing the chain service can do that harms users is report a
// wildly wrong fee rate. This file is the mitigation: the service applies
// these bounds before relaying a rate to the client. The client is then
// responsible for the "explicit confirmation" half — requiring the user to
// confirm anything above WarningThresholdSatPerVB.
const (
	// MinSatPerVB is the minimum fee rate the service will relay.
	//
	// 1 sat/vB is the network relay minimum. A rate below this will not
	// propagate; reporting it would cause a transaction to sit unconfirmed
	// indefinitely. The service refuses to relay rates below this floor.
	MinSatPerVB = 1

	// MaxSatPerVB is the maximum fee rate the service will accept from a
	// provider before treating the response as suspicious and returning an
	// error.
	//
	// 2000 sat/vB is orders of magnitude above any fee ever paid in practice
	// (peak congestion has reached ~500). Anything above this is almost
	// certainly a provider bug or an attack. Relaying it would cause the user
	// to pay enormously more than expected; refusing it is the safe choice.
	MaxSatPerVB = 2000

	// WarningThresholdSatPerVB is the rate above which the client must show
	// the user an explicit confirmation screen.
	//
	// 200 sat/vB is well above normal congestion peaks and signals an
	// extraordinary network event. The chain service flags rates above this so
	// the app can pause before broadcasting.
	WarningThresholdSatPerVB = 200
)

// FeeRate is a fee rate in satoshis per virtual byte, validated to be within
// the plausible range defined above.
type FeeRate struct {
	// SatPerVB is the fee rate in satoshis per virtual byte.
	SatPerVB float64

	// RequiresConfirmation is true when the rate exceeds WarningThresholdSatPerVB.
	//
	// The mobile app must show an explicit confirmation screen before
	// broadcasting any transaction built at this rate. This is the "explicit
	// confirmation for anything unusual" that A.14 mandates.
	RequiresConfirmation bool

	// ConfirmationReason is a human-readable explanation for why confirmation
	// is required. Empty when RequiresConfirmation is false.
	ConfirmationReason string
}

// ValidateFeeRate applies the A.14 sanity bounds to a raw rate from a provider.
//
// Returns an error if the rate is below MinSatPerVB or above MaxSatPerVB.
// Sets RequiresConfirmation and ConfirmationReason if the rate is above
// WarningThresholdSatPerVB.
//
// The caller should not relay an error to the client as a fee rate; an error
// here means the provider response is suspicious.
func ValidateFeeRate(satPerVB float64) (FeeRate, error) {
	if satPerVB < MinSatPerVB {
		return FeeRate{}, fmt.Errorf(
			"fee rate %.1f sat/vB is below the relay minimum (%d sat/vB); provider response is suspect",
			satPerVB, MinSatPerVB,
		)
	}
	if satPerVB > MaxSatPerVB {
		return FeeRate{}, fmt.Errorf(
			"fee rate %.1f sat/vB exceeds the plausible maximum (%d sat/vB); provider response is suspect",
			satPerVB, MaxSatPerVB,
		)
	}

	f := FeeRate{SatPerVB: satPerVB}
	if satPerVB > WarningThresholdSatPerVB {
		f.RequiresConfirmation = true
		f.ConfirmationReason = fmt.Sprintf(
			"fee rate %.0f sat/vB is unusually high — confirm you want to pay this rate",
			satPerVB,
		)
	}
	return f, nil
}

// FeeEstimates holds fee rate estimates for different confirmation targets.
//
// All values have already passed through ValidateFeeRate and are within the
// plausible range. Any provider estimate that failed validation is replaced
// by the nearest valid value and the incident is logged, rather than
// returning a partial or errored response — the client always gets something
// it can use.
type FeeEstimates struct {
	// Fast targets roughly 1-2 block confirmation.
	Fast FeeRate
	// Medium targets roughly 3-6 block confirmation.
	Medium FeeRate
	// Economy targets roughly 10+ block confirmation.
	Economy FeeRate
}
