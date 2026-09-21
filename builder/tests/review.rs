//! §4, enforced: the review describes the transaction, not the request.
//!
//! The rule is already structural — `ReviewSummary::derive` has no `Intent`
//! parameter, so summarising the request instead of the payload is not
//! expressible. These tests check the consequence that actually matters: if the
//! transaction changes, the summary changes with it.
//!
//! That is what makes §4 worth having. A summary generated from intent would
//! show a correct-looking approval screen for a transaction that pays somewhere
//! else — which is the exact shape of a compromised builder, and the reason
//! "verify before sign" is a rule rather than a nicety.

mod common;

use bitcoin::{Amount, TxOut};
use common::{address, intent, utxos};
use thadam_builder::{build, review::ReviewSummary};
use thadam_registry::BitcoinNetwork;

const NETWORK: BitcoinNetwork = BitcoinNetwork::Testnet4;

fn built() -> thadam_builder::Payload {
    build(&utxos(NETWORK, &[5_000_000]), &intent(NETWORK, 100_000, 5)).expect("must build")
}

/// Redirecting an output changes what the summary reports.
///
/// The attack this models: something between the builder and the signer swaps
/// the recipient. If the summary came from intent, the user would approve the
/// original recipient and sign the substituted one.
#[test]
fn changing_a_recipient_changes_the_summary() {
    let payload = built();
    let change_address = address(NETWORK, 1);

    let mut tampered = payload.transaction.clone();
    let attacker = address(NETWORK, 99);
    tampered.output[0].script_pubkey = attacker.script_pubkey().unwrap();

    let input_values: Vec<Amount> = payload.selected.iter().map(|u| u.value).collect();
    let after = ReviewSummary::derive(&tampered, &input_values, NETWORK, &change_address)
        .expect("tampered transaction still summarises");

    assert_ne!(
        payload.review.outputs[0].address, after.outputs[0].address,
        "the summary did not notice the recipient was swapped"
    );
    assert_eq!(
        after.outputs[0].address.as_ref().map(|a| a.to_string()),
        Some(attacker.to_string()),
        "the summary must report where the money actually goes"
    );
    assert_ne!(payload.review.txid, after.txid);
}

/// Moving value from change to the recipient changes the reported amounts.
///
/// Deliberately value-conserving: totals and fee are untouched, so a review
/// that only checked "does the fee look sane" would see nothing wrong. What
/// changed is how much leaves the wallet, which is the number the user actually
/// cares about.
#[test]
fn moving_value_from_change_to_the_recipient_changes_the_summary() {
    let payload = built();
    let change_address = address(NETWORK, 1);
    let input_values: Vec<Amount> = payload.selected.iter().map(|u| u.value).collect();

    let stolen = Amount::from_sat(50_000);
    let mut tampered = payload.transaction.clone();
    tampered.output[0].value += stolen;
    tampered.output[1].value -= stolen;

    let after = ReviewSummary::derive(&tampered, &input_values, NETWORK, &change_address).unwrap();

    // The fee is unchanged — this attack is invisible to a fee check.
    assert_eq!(after.fee, payload.review.fee);
    assert_eq!(after.total_output, payload.review.total_output);

    // But the summary reports the real amounts.
    assert_eq!(after.outputs[0].amount, payload.review.outputs[0].amount + stolen);
    assert_eq!(
        after.amount_leaving_the_wallet(),
        payload.review.amount_leaving_the_wallet() + stolen,
        "the summary must show that more is leaving the wallet"
    );
    assert_ne!(after.txid, payload.review.txid);
}

/// An extra output nobody asked for appears in the summary.
#[test]
fn an_added_output_appears_in_the_summary() {
    let payload = built();
    let change_address = address(NETWORK, 1);
    let input_values: Vec<Amount> = payload.selected.iter().map(|u| u.value).collect();

    // Funded by shrinking change, so totals still balance and the fee is
    // untouched. The extra output is the whole of the attack.
    let siphoned = Amount::from_sat(10_000);
    let mut tampered = payload.transaction.clone();
    tampered.output[1].value -= siphoned;
    tampered.output.push(TxOut {
        value: siphoned,
        script_pubkey: address(NETWORK, 99).script_pubkey().unwrap(),
    });

    let after = ReviewSummary::derive(&tampered, &input_values, NETWORK, &change_address).unwrap();

    assert_eq!(after.outputs.len(), payload.review.outputs.len() + 1);
    assert_eq!(after.fee, payload.review.fee, "the fee alone reveals nothing");
    assert_eq!(
        after.amount_leaving_the_wallet(),
        payload.review.amount_leaving_the_wallet() + siphoned,
        "an added output must count as money leaving"
    );
    assert!(!after.outputs[2].is_change, "the attacker's output is not change");
}

/// Change is identified by script, not by position.
///
/// A builder bug that put change first, or an attacker who reordered outputs,
/// must not turn a payment into "change" in the user's eyes.
#[test]
fn change_is_identified_by_script_not_position() {
    let payload = built();
    let change_address = address(NETWORK, 1);
    let input_values: Vec<Amount> = payload.selected.iter().map(|u| u.value).collect();

    let mut reordered = payload.transaction.clone();
    reordered.output.reverse();

    let after = ReviewSummary::derive(&reordered, &input_values, NETWORK, &change_address).unwrap();

    assert_eq!(after.outputs.len(), 2);
    assert!(after.outputs[0].is_change, "change moved but was still found");
    assert!(!after.outputs[1].is_change);
    assert_eq!(
        after.amount_leaving_the_wallet(),
        payload.review.amount_leaving_the_wallet(),
        "reordering must not change what the user is told they are spending"
    );
}

/// A transaction paying out more than it takes in cannot be summarised.
///
/// It is invalid and would print money. Returning a summary for it — with a
/// negative or wrapped fee — would be worse than refusing.
#[test]
fn a_transaction_that_prints_money_is_refused() {
    let payload = built();
    let change_address = address(NETWORK, 1);
    let input_values: Vec<Amount> = payload.selected.iter().map(|u| u.value).collect();

    let mut tampered = payload.transaction.clone();
    tampered.output[0].value = Amount::from_sat(99_000_000);

    assert!(
        ReviewSummary::derive(&tampered, &input_values, NETWORK, &change_address).is_err(),
        "summarised a transaction whose outputs exceed its inputs"
    );
}

/// Wrong input values are refused rather than producing a wrong fee.
///
/// Bitcoin's fee is implicit — inputs minus outputs — so a caller that supplies
/// the wrong values gets a wrong fee with no error anywhere. The count check is
/// the only part that can be verified from the transaction itself.
#[test]
fn a_mismatched_input_count_is_refused() {
    let payload = built();
    let change_address = address(NETWORK, 1);

    assert!(
        ReviewSummary::derive(&payload.transaction, &[], NETWORK, &change_address).is_err(),
        "accepted zero input values for a transaction with inputs"
    );
    assert!(
        ReviewSummary::derive(
            &payload.transaction,
            &[Amount::from_sat(1), Amount::from_sat(2)],
            NETWORK,
            &change_address
        )
        .is_err(),
        "accepted two values for a one-input transaction"
    );
}

/// The txid shown at review is the txid that lands on chain.
///
/// For an all-segwit transaction the txid does not cover the witness, so
/// signing does not change it. What the user approves is findable afterwards,
/// which is what makes a review screen auditable rather than decorative.
#[test]
fn the_reviewed_txid_is_stable_under_signing() {
    let payload = built();
    let change_address = address(NETWORK, 1);
    let input_values: Vec<Amount> = payload.selected.iter().map(|u| u.value).collect();

    // Stand in for the signer filling the witness.
    let mut signed = payload.transaction.clone();
    for input in &mut signed.input {
        input.witness.push([0x30u8; 72]);
        input.witness.push([0x02u8; 33]);
    }

    assert_eq!(
        signed.compute_txid(),
        payload.review.txid,
        "adding witnesses changed the txid"
    );

    let after = ReviewSummary::derive(&signed, &input_values, NETWORK, &change_address).unwrap();
    assert_eq!(after.txid, payload.review.txid);
    assert_eq!(after.outputs, payload.review.outputs);
}
