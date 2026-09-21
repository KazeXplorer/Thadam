//! Thadam transaction builder.
//!
//! ARCHITECTURE.md §3: takes coins and intent, returns an unsigned payload plus
//! a review summary. **Deterministic. No network access. No key access.**
//!
//! Those three constraints are load-bearing, not stylistic:
//!
//! - *No keys* — this crate sits above the signer and must never see one. It
//!   cannot sign, and nothing here accepts a private key.
//! - *No network* — so a transaction can be built and reviewed offline, and so
//!   the review cannot be influenced by anything arriving mid-flight.
//! - *Deterministic* — the same coins and the same intent give the same
//!   transaction every time. Without this, §4's review describes something that
//!   might differ from what gets signed a moment later.
//!
//! # What the user actually approves
//!
//! [`Payload::review`] is derived from the transaction bytes, never from the
//! intent — see [`review`]. The rule is enforced by the shape of the code
//! rather than by care.
//!
//! # What is not here
//!
//! Signing, broadcast, and anything needing the chain's current state. Fee rate
//! is an input to this crate; discovering a sensible one is the chain service's
//! job, and that does not exist yet.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod fee;
pub mod review;
pub mod select;
pub mod utxo;

use bitcoin::{absolute::LockTime, transaction::Version, Amount, Sequence};
use bitcoin::{Transaction, TxIn, TxOut, Witness};
use thadam_registry::BitcoinNetwork;

pub use review::{ReviewOutput, ReviewSummary};
pub use select::Strategy;
pub use utxo::{Intent, Utxo};

/// An unsigned transaction and the summary the user approves before signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// The unsigned transaction.
    pub transaction: Transaction,
    /// What it does, read back out of those bytes.
    pub review: ReviewSummary,
    /// Which coins it spends, in input order.
    pub selected: Vec<Utxo>,
    /// How the coins were chosen.
    pub strategy: Strategy,
    /// The network every part of this belongs to.
    pub network: BitcoinNetwork,
}

/// Why a transaction could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No coins were offered.
    NoCoins,
    /// The coins, the recipient and the change address are not all on one
    /// network.
    ///
    /// §8: addresses do not identify their network, so this cannot be inferred
    /// from the strings. It is checked because the types carry it.
    MixedNetworks,
    /// The requested amount is below the dust threshold and could not be
    /// relayed.
    AmountIsDust {
        /// What was requested.
        requested: Amount,
        /// The smallest relayable amount.
        threshold: Amount,
    },
    /// Coin selection failed.
    Selection(select::Error),
    /// An address could not be turned into an output script.
    Address(thadam_signer::Error),
    /// The summary could not be derived from the transaction that was built.
    Review(review::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoCoins => f.write_str("no coins to spend"),
            Error::MixedNetworks => {
                f.write_str("coins, recipient and change address are not all on one network")
            }
            Error::AmountIsDust {
                requested,
                threshold,
            } => write!(f, "{requested} is below the dust threshold of {threshold}"),
            Error::Selection(e) => write!(f, "{e}"),
            Error::Address(e) => write!(f, "{e}"),
            Error::Review(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<select::Error> for Error {
    fn from(e: select::Error) -> Self {
        Error::Selection(e)
    }
}

impl From<thadam_signer::Error> for Error {
    fn from(e: thadam_signer::Error) -> Self {
        Error::Address(e)
    }
}

impl From<review::Error> for Error {
    fn from(e: review::Error) -> Self {
        Error::Review(e)
    }
}

/// Builds an unsigned transaction paying `intent.amount` to `intent.recipient`.
///
/// # How the fee is arrived at
///
/// Bitcoin charges by size, the size depends on how many coins get selected,
/// and how many get selected depends on the fee. That circularity is broken by
/// working in *effective values*: each coin's own input fee is subtracted up
/// front, so selection only has to cover the amount plus the fee for the parts
/// of the transaction that do not depend on selection.
///
/// # When change disappears
///
/// If what is left over is below the dust threshold, a change output would be
/// unrelayable. It is dropped and the leftover becomes extra fee — which is why
/// the review reports an *effective* fee rate rather than the requested one.
/// The user overpays knowingly, because the alternative is a transaction the
/// network refuses.
pub fn build(utxos: &[Utxo], intent: &Intent) -> Result<Payload, Error> {
    if utxos.is_empty() {
        return Err(Error::NoCoins);
    }

    let network = intent.recipient.network();
    if intent.change_address.network() != network
        || utxos.iter().any(|utxo| utxo.address.network() != network)
    {
        return Err(Error::MixedNetworks);
    }

    let dust = fee::dust_threshold();
    if intent.amount < dust {
        return Err(Error::AmountIsDust {
            requested: intent.amount,
            threshold: dust,
        });
    }

    let recipient_script = intent.recipient.script_pubkey()?;
    let change_script = intent.change_address.script_pubkey()?;

    // Selection must cover the payment plus the fee for everything that is not
    // an input. Input fees already sit inside each coin's effective value.
    let overhead_vsize = fee::predicted_vsize(0, 1);
    let target = intent.amount + fee::fee_for_vsize(intent.fee_rate, overhead_vsize);

    let selection = select::select(utxos, target, intent.fee_rate)?;
    let selected: Vec<Utxo> = selection
        .indices
        .iter()
        .map(|&index| utxos[index].clone())
        .collect();

    let total_input: Amount = selected.iter().map(|utxo| utxo.value).sum();

    let mut outputs = vec![TxOut {
        value: intent.amount,
        script_pubkey: recipient_script,
    }];

    // Whether to make change is decided by the strategy, not recomputed here.
    //
    // Branch-and-bound's entire purpose is to find a selection that does not
    // need change: the overshoot it accepts is, by construction, smaller than
    // what creating and later spending a change output would cost. Recomputing
    // a change amount at this point and finding it above dust would add back
    // the very output the search was run to avoid — a transaction that is
    // larger, more expensive, and leaves a coin to manage.
    match selection.strategy {
        Strategy::BranchAndBound => {
            // The overshoot becomes fee. That is the cheaper outcome and the
            // reason the window has the width it does.
        }
        Strategy::LargestFirst => {
            let vsize_with_change = fee::predicted_vsize(selected.len(), 2);
            let fee_with_change = fee::fee_for_vsize(intent.fee_rate, vsize_with_change);

            let change = total_input
                .checked_sub(intent.amount)
                .and_then(|rest| rest.checked_sub(fee_with_change));

            if let Some(change) = change {
                if change >= dust {
                    outputs.push(TxOut {
                        value: change,
                        script_pubkey: change_script,
                    });
                }
            }
        }
    }

    let transaction = Transaction {
        // Version 2 enables relative timelocks (BIP-68). Nothing here uses
        // them, but version 1 has no advantage and 2 is what current wallets
        // emit.
        version: Version::TWO,
        // Zero means valid from any height. Some wallets set the current height
        // as an anti-fee-sniping measure; that needs chain state, which §3
        // forbids this layer from having.
        lock_time: LockTime::ZERO,
        input: selected
            .iter()
            .map(|utxo| TxIn {
                previous_output: utxo.outpoint,
                // Empty: a segwit input's signature lives in the witness, and
                // the witness is the signer's to fill.
                script_sig: bitcoin::ScriptBuf::new(),
                // RBF: 0xFFFFFFFD signals replace-by-fee without enabling
                // relative locktime. This is deliberate — without it, a stuck
                // transaction (mempool congestion, fee-rate misjudgement) cannot
                // be fee-bumped, and the only recovery is to wait indefinitely.
                // §11 records the decision.
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            })
            .collect(),
        output: outputs,
    };

    let input_values: Vec<Amount> = selected.iter().map(|utxo| utxo.value).collect();
    let review = ReviewSummary::derive(
        &transaction,
        &input_values,
        network,
        &intent.change_address,
    )?;

    Ok(Payload {
        transaction,
        review,
        selected,
        strategy: selection.strategy,
        network,
    })
}
