//! Properties every transaction this crate builds must have.
//!
//! These are written as properties over many random inputs rather than as a
//! handful of examples, because the failures that matter here are arithmetic:
//! an off-by-one in the fee, a change output that is a satoshi under dust, a
//! rounding step that loses value. Those hide in specific numbers, and specific
//! numbers are what a fixed example set does not have.

mod common;

use bitcoin::{Amount, FeeRate};
use common::{intent, utxos};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use thadam_builder::{build, fee, Strategy};
use thadam_registry::BitcoinNetwork;

const NETWORK: BitcoinNetwork = BitcoinNetwork::Testnet4;

/// Fixed seed: a failure must be reproducible from the test output alone.
fn rng() -> StdRng {
    StdRng::seed_from_u64(0xb1_11de5)
}

/// Value is conserved. Inputs equal outputs plus fee, exactly, always.
///
/// The most important property in the file. Bitcoin does not record a fee — it
/// is whatever the inputs exceed the outputs by — so an arithmetic slip here
/// does not produce an error, it produces a transaction that hands a miner the
/// difference.
#[test]
fn inputs_equal_outputs_plus_fee() {
    let mut rng = rng();

    for _ in 0..400 {
        let coins: Vec<u64> = (0..rng.gen_range(1..8))
            .map(|_| rng.gen_range(1_000..2_000_000))
            .collect();
        let available: u64 = coins.iter().sum();
        if available < 100_000 {
            continue;
        }
        let amount = rng.gen_range(1_000..available / 2);
        let rate = rng.gen_range(1..40);

        let utxos = utxos(NETWORK, &coins);
        let Ok(payload) = build(&utxos, &intent(NETWORK, amount, rate)) else {
            continue;
        };

        let total_in: Amount = payload.selected.iter().map(|u| u.value).sum();
        let total_out: Amount = payload.transaction.output.iter().map(|o| o.value).sum();

        assert_eq!(
            total_in,
            total_out + payload.review.fee,
            "value not conserved: coins={coins:?} amount={amount} rate={rate}"
        );
        assert_eq!(payload.review.total_input, total_in);
        assert_eq!(payload.review.total_output, total_out);
    }
}

/// The transaction always pays at least the fee rate that was asked for.
///
/// Underpaying is the one direction that cannot be recovered from: the
/// transaction sits in the mempool and there is no cancel button. Overpaying is
/// permitted and happens whenever dust change is absorbed into the fee.
#[test]
fn the_requested_fee_rate_is_always_met_or_exceeded() {
    let mut rng = rng();

    for _ in 0..400 {
        let coins: Vec<u64> = (0..rng.gen_range(1..6))
            .map(|_| rng.gen_range(50_000..5_000_000))
            .collect();
        let available: u64 = coins.iter().sum();
        let amount = rng.gen_range(1_000..available / 2);
        let rate = rng.gen_range(1..40);

        let utxos = utxos(NETWORK, &coins);
        let Ok(payload) = build(&utxos, &intent(NETWORK, amount, rate)) else {
            continue;
        };

        let required = fee::fee_for_vsize(
            FeeRate::from_sat_per_vb(rate).unwrap(),
            payload.review.vsize,
        );

        assert!(
            payload.review.fee >= required,
            "underpaid: fee={} required={required} vsize={} coins={coins:?} amount={amount} rate={rate}",
            payload.review.fee,
            payload.review.vsize
        );
    }
}

/// No output is ever below the dust threshold.
///
/// A dust output makes the whole transaction non-standard, so the network drops
/// it. The failure is silent from the wallet's point of view: it builds fine,
/// signs fine, and simply never confirms.
#[test]
fn no_output_is_ever_dust() {
    let mut rng = rng();
    let dust = fee::dust_threshold();

    for _ in 0..600 {
        let coins: Vec<u64> = (0..rng.gen_range(1..5))
            .map(|_| rng.gen_range(1_000..500_000))
            .collect();
        let available: u64 = coins.iter().sum();
        // Deliberately skew toward amounts that leave almost nothing over, so
        // the near-dust change boundary is actually exercised.
        let amount = rng.gen_range(300..available.max(301));
        let rate = rng.gen_range(1..30);

        let utxos = utxos(NETWORK, &coins);
        let Ok(payload) = build(&utxos, &intent(NETWORK, amount, rate)) else {
            continue;
        };

        for output in &payload.transaction.output {
            assert!(
                output.value >= dust,
                "dust output {} (threshold {dust}) coins={coins:?} amount={amount} rate={rate}",
                output.value
            );
        }
    }
}

/// The recipient receives exactly what was asked for.
///
/// Fees come out of change, never out of the payment. A wallet that quietly
/// shaves the fee off the recipient's amount is a wallet that sends the wrong
/// number.
#[test]
fn the_recipient_receives_the_exact_amount() {
    let mut rng = rng();

    for _ in 0..300 {
        let coins: Vec<u64> = (0..rng.gen_range(1..6))
            .map(|_| rng.gen_range(100_000..3_000_000))
            .collect();
        let amount = rng.gen_range(1_000..200_000);
        let rate = rng.gen_range(1..25);

        let utxos = utxos(NETWORK, &coins);
        let Ok(payload) = build(&utxos, &intent(NETWORK, amount, rate)) else {
            continue;
        };

        assert_eq!(
            payload.review.amount_leaving_the_wallet(),
            Amount::from_sat(amount),
            "recipient amount changed: coins={coins:?} rate={rate}"
        );
    }
}

/// §3: deterministic. The same coins and intent give the same transaction.
///
/// Without this the review in §4 describes something that might not be what
/// gets signed a moment later.
#[test]
fn building_twice_gives_an_identical_transaction() {
    let mut rng = rng();

    for _ in 0..100 {
        let coins: Vec<u64> = (0..rng.gen_range(2..7))
            .map(|_| rng.gen_range(10_000..1_000_000))
            .collect();
        let amount = rng.gen_range(5_000..100_000);
        let rate = rng.gen_range(1..20);

        let utxos = utxos(NETWORK, &coins);
        let intent = intent(NETWORK, amount, rate);

        let Ok(first) = build(&utxos, &intent) else {
            continue;
        };
        let second = build(&utxos, &intent).expect("second build must also succeed");

        assert_eq!(first, second, "build is not deterministic: coins={coins:?}");
    }
}

/// A changeless solution, when one exists, really has one output.
#[test]
fn branch_and_bound_produces_no_change_output() {
    // One coin sized so that, after its input fee, it lands inside the
    // cost-of-change window above the target.
    let rate = FeeRate::from_sat_per_vb(5).unwrap();
    let amount = 100_000u64;
    let overhead = fee::fee_for_vsize(rate, fee::predicted_vsize(0, 1));
    let input_fee = fee::input_fee(rate);
    let exact = amount + overhead.to_sat() + input_fee.to_sat();

    let utxos = utxos(NETWORK, &[exact]);
    let payload = build(&utxos, &intent(NETWORK, amount, 5)).expect("must build");

    assert_eq!(payload.strategy, Strategy::BranchAndBound);
    assert_eq!(payload.transaction.output.len(), 1, "expected no change");
    assert!(!payload.review.has_change());
}

/// When change is kept, it goes to the change address and is marked as change.
#[test]
fn change_is_recognised_as_change() {
    let utxos = utxos(NETWORK, &[5_000_000]);
    let payload = build(&utxos, &intent(NETWORK, 100_000, 5)).expect("must build");

    assert_eq!(payload.transaction.output.len(), 2);
    assert!(payload.review.has_change());

    let change: Vec<_> = payload.review.outputs.iter().filter(|o| o.is_change).collect();
    assert_eq!(change.len(), 1);
    assert_eq!(
        change[0].address.as_ref().map(|a| a.to_string()),
        Some(common::address(NETWORK, 1).to_string())
    );
}

/// Change below dust is absorbed into the fee, and the review says so.
#[test]
fn dust_change_becomes_fee_and_the_effective_rate_rises() {
    let rate_vb = 5u64;
    let rate = FeeRate::from_sat_per_vb(rate_vb).unwrap();
    let amount = 100_000u64;

    // Size the coin so the leftover after a two-output fee is just under dust.
    let two_output_fee = fee::fee_for_vsize(rate, fee::predicted_vsize(1, 2));
    let coin = amount + two_output_fee.to_sat() + fee::dust_threshold().to_sat() - 1;

    let utxos = utxos(NETWORK, &[coin]);
    let payload = build(&utxos, &intent(NETWORK, amount, rate_vb)).expect("must build");

    assert_eq!(payload.transaction.output.len(), 1, "dust change must be dropped");
    assert!(!payload.review.has_change());
    assert!(
        payload.review.effective_fee_rate > rate,
        "effective rate {} should exceed requested {rate}",
        payload.review.effective_fee_rate
    );
}

/// Change exactly equal to the dust threshold is kept, not dropped.
///
/// Added after a mutation test: changing `>= dust` to `> dust` survived the
/// whole suite. The boundary is where off-by-ones live, and nothing was
/// standing on it.
#[test]
fn change_exactly_at_the_dust_threshold_is_kept() {
    // Rate 4, not 5. At higher rates the input fee exceeds the dust threshold,
    // so a leftover of exactly dust lands inside branch-and-bound's window and
    // the transaction comes out changeless — there is no change to sit on the
    // boundary. The dust rule only applies on the largest-first path, and the
    // test asserts it took that path.
    let rate_vb = 4u64;
    let rate = FeeRate::from_sat_per_vb(rate_vb).unwrap();
    let amount = 100_000u64;
    let dust = fee::dust_threshold();

    let two_output_fee = fee::fee_for_vsize(rate, fee::predicted_vsize(1, 2));
    let coin = amount + two_output_fee.to_sat() + dust.to_sat();

    let utxos = utxos(NETWORK, &[coin]);
    let payload = build(&utxos, &intent(NETWORK, amount, rate_vb)).expect("must build");

    assert_eq!(payload.strategy, Strategy::LargestFirst, "wrong path for this test");
    assert_eq!(
        payload.transaction.output.len(),
        2,
        "change of exactly {dust} is relayable and must be kept"
    );
    assert!(payload.review.has_change());
    let change = payload.review.outputs.iter().find(|o| o.is_change).unwrap();
    assert_eq!(change.amount, dust);
}

/// One satoshi below the dust threshold, change is dropped.
///
/// The other side of the same boundary.
#[test]
fn change_one_satoshi_below_dust_is_dropped() {
    let rate_vb = 4u64;
    let rate = FeeRate::from_sat_per_vb(rate_vb).unwrap();
    let amount = 100_000u64;

    let two_output_fee = fee::fee_for_vsize(rate, fee::predicted_vsize(1, 2));
    let coin = amount + two_output_fee.to_sat() + fee::dust_threshold().to_sat() - 1;

    let payload =
        build(&utxos(NETWORK, &[coin]), &intent(NETWORK, amount, rate_vb)).expect("must build");

    assert_eq!(payload.strategy, Strategy::LargestFirst, "wrong path for this test");
    assert_eq!(payload.transaction.output.len(), 1);
}

/// The branch-and-bound window is exactly `cost_of_change` wide.
///
/// Also added after a mutation test: dropping the future-spend half of
/// `cost_of_change` narrowed the window and nothing noticed. That half is not
/// decoration — leaving it out means the wallet makes change in cases where
/// paying the overshoot to a miner would have been cheaper, and accumulates
/// coins it will pay again to spend.
#[test]
fn the_changeless_window_is_as_wide_as_the_cost_of_change() {
    let rate_vb = 10u64;
    let rate = FeeRate::from_sat_per_vb(rate_vb).unwrap();
    let amount = 100_000u64;

    let overhead = fee::fee_for_vsize(rate, fee::predicted_vsize(0, 1)).to_sat();
    let input_fee = fee::input_fee(rate).to_sat();
    let window = fee::cost_of_change(rate).to_sat();
    let exact = amount + overhead + input_fee;

    // Just inside the far edge of the window: still changeless.
    let inside = build(
        &utxos(NETWORK, &[exact + window - 1]),
        &intent(NETWORK, amount, rate_vb),
    )
    .expect("must build");
    assert_eq!(
        inside.strategy,
        Strategy::BranchAndBound,
        "a coin overshooting by {} should still be changeless (window {window})",
        window - 1
    );
    assert_eq!(inside.transaction.output.len(), 1);

    // The window must be wide enough that the future-spend half matters: a coin
    // overshooting by more than one output's fee, but less than the full cost of
    // change, must still be changeless.
    let output_fee_only = fee::fee_for_vsize(rate, fee::vsize_per_output()).to_sat();
    assert!(
        window > output_fee_only,
        "cost_of_change ({window}) must include the future spend, not just the output ({output_fee_only})"
    );
    let between = build(
        &utxos(NETWORK, &[exact + output_fee_only + 1]),
        &intent(NETWORK, amount, rate_vb),
    )
    .expect("must build");
    assert_eq!(
        between.strategy,
        Strategy::BranchAndBound,
        "overshoot of {} is inside the full window but outside a create-only one",
        output_fee_only + 1
    );
}

/// Every input must signal RBF (sequence ≤ 0xFFFFFFFD).
///
/// Without RBF, a transaction stuck in the mempool due to congestion or a
/// fee-rate misjudgement cannot be fee-bumped — the wallet is frozen until
/// miners confirm it or the mempool drops it. That can be hours or days.
/// This test verifies the builder never produces an input that opts out.
#[test]
fn every_input_signals_rbf() {
    let mut rng = rng();

    for _ in 0..200 {
        let coins: Vec<u64> = (0..rng.gen_range(1..6))
            .map(|_| rng.gen_range(50_000..5_000_000))
            .collect();
        let available: u64 = coins.iter().sum();
        let amount = rng.gen_range(1_000..available / 2);
        let rate = rng.gen_range(1..40);

        let utxos = utxos(NETWORK, &coins);
        let Ok(payload) = build(&utxos, &intent(NETWORK, amount, rate)) else {
            continue;
        };

        for (i, input) in payload.transaction.input.iter().enumerate() {
            assert!(
                input.sequence.0 <= 0xFFFF_FFFD,
                "input[{i}] does not signal RBF: sequence=0x{:08X} coins={coins:?} amount={amount} rate={rate}",
                input.sequence.0
            );
        }
    }
}
