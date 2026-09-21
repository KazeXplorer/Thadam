//! What the user is asked to approve.
//!
//! ARCHITECTURE.md §4: *the review summary is derived from the actual payload
//! bytes, never from the intent that produced them.*
//!
//! # How that rule is enforced here
//!
//! [`ReviewSummary::derive`] does not take an [`Intent`](crate::Intent). It
//! cannot. There is no parameter to pass one through, so deriving the summary
//! from what the user asked for instead of from what was built is not a
//! discipline someone has to remember — it is not expressible.
//!
//! This matters because the two can differ, and every way they differ is a way
//! a user loses money. A builder bug that sends to the wrong script. A change
//! address that is not actually the wallet's. A fee that swallowed a dust
//! change output and is now ten times what was asked for. Summarising the
//! intent would show the user a correct-looking screen describing a transaction
//! that does not exist.
//!
//! Every field below is read back out of the transaction. The only thing passed
//! in alongside is the value of each input, because that is genuinely not in an
//! unsigned Bitcoin transaction — inputs reference outputs by hash, they do not
//! restate their amounts.

use bitcoin::{Address as LibAddress, Amount, FeeRate, Transaction, Txid};
use thadam_registry::BitcoinNetwork;
use thadam_signer::Address;

/// One output, as it actually appears in the transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewOutput {
    /// Where it pays, decoded from the output script.
    ///
    /// `None` when the script is not one this build can express as an address.
    /// Shown to the user as an unrecognised script rather than hidden — an
    /// output nobody can describe is exactly the one worth looking at.
    pub address: Option<Address>,
    /// How much it pays.
    pub amount: Amount,
    /// Whether this output pays back to the address change was meant to go to.
    ///
    /// Determined by comparing scripts, not by position or by trusting the
    /// builder's own bookkeeping.
    pub is_change: bool,
}

/// What the user approves, read out of the transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSummary {
    /// The transaction this describes.
    ///
    /// For an all-segwit transaction the txid does not cover the witness, so
    /// this is the same id the transaction will have once signed and mined.
    /// What the user reviews is identifiable on-chain afterwards.
    pub txid: Txid,
    /// Every output, in transaction order.
    pub outputs: Vec<ReviewOutput>,
    /// What is being spent, summed from the supplied input values.
    pub total_input: Amount,
    /// What the outputs carry.
    pub total_output: Amount,
    /// Input minus output. In Bitcoin the fee is never written down — it is
    /// whatever is left over, which is why a wrong input value produces a wrong
    /// fee and why an unsigned transaction cannot state its own.
    pub fee: Amount,
    /// Predicted virtual size once signed.
    pub vsize: u64,
    /// The fee rate this transaction actually achieves.
    ///
    /// Not the requested rate. When change would have been dust it is dropped
    /// into the fee instead, and this is where the user sees that happen.
    pub effective_fee_rate: FeeRate,
    /// How many coins are being spent.
    pub input_count: usize,
}

/// Why a summary could not be derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// An input value was supplied for a different number of inputs than the
    /// transaction has.
    InputCountMismatch {
        /// Inputs in the transaction.
        transaction: usize,
        /// Values supplied.
        supplied: usize,
    },
    /// Outputs total more than inputs. The transaction is invalid and would
    /// print money; refusing to summarise it is the only safe answer.
    OutputsExceedInputs {
        /// Total of the supplied input values.
        total_input: Amount,
        /// Total of the transaction's outputs.
        total_output: Amount,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InputCountMismatch {
                transaction,
                supplied,
            } => write!(
                f,
                "transaction has {transaction} inputs but {supplied} values were supplied"
            ),
            Error::OutputsExceedInputs {
                total_input,
                total_output,
            } => write!(f, "outputs ({total_output}) exceed inputs ({total_input})"),
        }
    }
}

impl std::error::Error for Error {}

impl ReviewSummary {
    /// Reads a transaction and describes it.
    ///
    /// Note the parameters: a transaction, its input values, the network, and
    /// the change address to recognise. **No intent.** See this module's
    /// documentation for why that absence is the point.
    pub fn derive(
        transaction: &Transaction,
        input_values: &[Amount],
        network: BitcoinNetwork,
        change_address: &Address,
    ) -> Result<Self, Error> {
        if transaction.input.len() != input_values.len() {
            return Err(Error::InputCountMismatch {
                transaction: transaction.input.len(),
                supplied: input_values.len(),
            });
        }

        let total_input: Amount = input_values.iter().copied().sum();
        let total_output: Amount = transaction.output.iter().map(|out| out.value).sum();

        if total_output > total_input {
            return Err(Error::OutputsExceedInputs {
                total_input,
                total_output,
            });
        }

        let change_script = change_address.script_pubkey().ok();

        let outputs = transaction
            .output
            .iter()
            .map(|out| ReviewOutput {
                address: decode_address(&out.script_pubkey, network),
                amount: out.value,
                is_change: change_script
                    .as_ref()
                    .is_some_and(|script| *script == out.script_pubkey),
            })
            .collect();

        let fee = total_input - total_output;
        let vsize = crate::fee::predicted_vsize(transaction.input.len(), transaction.output.len());
        let effective_fee_rate = FeeRate::from_sat_per_kwu(fee.to_sat() * 1000 / (vsize * 4).max(1));

        Ok(ReviewSummary {
            txid: transaction.compute_txid(),
            outputs,
            total_input,
            total_output,
            fee,
            vsize,
            effective_fee_rate,
            input_count: transaction.input.len(),
        })
    }

    /// What the recipient or recipients receive — everything that is not change.
    pub fn amount_leaving_the_wallet(&self) -> Amount {
        self.outputs
            .iter()
            .filter(|out| !out.is_change)
            .map(|out| out.amount)
            .sum()
    }

    /// Whether this transaction produces change.
    pub fn has_change(&self) -> bool {
        self.outputs.iter().any(|out| out.is_change)
    }
}

/// Turns an output script back into an address, if it is one we can express.
///
/// Goes through the signer's `Address`, which carries the network, so a script
/// decoded here cannot be shown without the network beside it (§8).
fn decode_address(script: &bitcoin::Script, network: BitcoinNetwork) -> Option<Address> {
    let library_network = LibAddress::from_script(script, network_params(network)).ok()?;
    Address::parse(&library_network.to_string(), network).ok()
}

/// The library's parameters for one of our networks.
///
/// A second conversion point, which README §2 cares about: every place a
/// network is translated is a place it could be translated wrongly. It is here
/// rather than in the signer because `from_script` needs `Params` rather than
/// `Network`, and it is held to the same rule — an exhaustive match with no
/// fallback arm, so a new network cannot be added without a decision at this
/// line. `tests/network_guard.rs` covers it.
fn network_params(network: BitcoinNetwork) -> &'static bitcoin::params::Params {
    match network {
        BitcoinNetwork::Mainnet => &bitcoin::params::Params::MAINNET,
        BitcoinNetwork::Testnet4 | BitcoinNetwork::Signet => &bitcoin::params::Params::SIGNET,
        BitcoinNetwork::Regtest => &bitcoin::params::Params::REGTEST,
    }
}
