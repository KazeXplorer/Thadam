package bitcoin_test

import (
	"testing"

	"thadam/chain/bitcoin"
)

// The fee sanity bounds are constants, not configuration. These tests pin their
// values so that a refactor cannot silently tighten or widen the range.
// Widening is dangerous (allows a lying provider to cost a user more);
// tightening silently breaks wallets during congestion events.

func TestFeeRateConstants(t *testing.T) {
	if bitcoin.MinSatPerVB != 1 {
		t.Errorf("MinSatPerVB = %d, want 1", bitcoin.MinSatPerVB)
	}
	if bitcoin.MaxSatPerVB != 2000 {
		t.Errorf("MaxSatPerVB = %d, want 2000", bitcoin.MaxSatPerVB)
	}
	if bitcoin.WarningThresholdSatPerVB != 200 {
		t.Errorf("WarningThresholdSatPerVB = %d, want 200", bitcoin.WarningThresholdSatPerVB)
	}
}

func TestValidateFeeRate(t *testing.T) {
	cases := []struct {
		name                string
		satPerVB            float64
		wantErr             bool
		wantConfirmation    bool
	}{
		// Normal range — no error, no confirmation.
		{"minimum relay rate", 1, false, false},
		{"typical low congestion", 10, false, false},
		{"typical moderate congestion", 50, false, false},
		// Just below the warning threshold — no confirmation.
		{"one below warning threshold", 199, false, false},
		// At and above the warning threshold — confirmation required.
		{"at warning threshold", 200, false, false},
		// 200 is the threshold itself; "unusual" starts above it.
		{"one above warning threshold", 201, false, true},
		{"high but plausible", 500, false, true},
		// At the absolute maximum — still valid but flagged.
		{"at maximum", 2000, false, true},
		// Below minimum and above maximum — errors.
		{"below minimum", 0.5, true, false},
		{"zero", 0, true, false},
		{"negative", -1, true, false},
		{"above maximum", 2001, true, false},
		{"extreme", 1_000_000, true, false},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f, err := bitcoin.ValidateFeeRate(tc.satPerVB)
			if (err != nil) != tc.wantErr {
				t.Fatalf("ValidateFeeRate(%.1f) error = %v, wantErr = %v", tc.satPerVB, err, tc.wantErr)
			}
			if err != nil {
				return
			}
			if f.SatPerVB != tc.satPerVB {
				t.Errorf("SatPerVB = %.1f, want %.1f", f.SatPerVB, tc.satPerVB)
			}
			if f.RequiresConfirmation != tc.wantConfirmation {
				t.Errorf("RequiresConfirmation = %v, want %v (rate %.1f)", f.RequiresConfirmation, tc.wantConfirmation, tc.satPerVB)
			}
			if f.RequiresConfirmation && f.ConfirmationReason == "" {
				t.Error("RequiresConfirmation is true but ConfirmationReason is empty")
			}
			if !f.RequiresConfirmation && f.ConfirmationReason != "" {
				t.Errorf("RequiresConfirmation is false but ConfirmationReason = %q", f.ConfirmationReason)
			}
		})
	}
}

// The warning threshold must be strictly inside the valid range.
// A threshold at or above MaxSatPerVB would mean confirmation is never
// requested; a threshold at or below MinSatPerVB would mean it is always
// requested for valid rates, which would train users to dismiss it.
func TestWarningThresholdIsInsidePlausibleRange(t *testing.T) {
	if bitcoin.WarningThresholdSatPerVB <= bitcoin.MinSatPerVB {
		t.Errorf("WarningThresholdSatPerVB (%d) must be > MinSatPerVB (%d)",
			bitcoin.WarningThresholdSatPerVB, bitcoin.MinSatPerVB)
	}
	if bitcoin.WarningThresholdSatPerVB >= bitcoin.MaxSatPerVB {
		t.Errorf("WarningThresholdSatPerVB (%d) must be < MaxSatPerVB (%d)",
			bitcoin.WarningThresholdSatPerVB, bitcoin.MaxSatPerVB)
	}
}
