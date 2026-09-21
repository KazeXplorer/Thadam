//! Coin selection.
//!
//! This is the part of a Bitcoin wallet that has no equivalent in an
//! account-model chain. Ethereum has a balance and you subtract from it.
//! Bitcoin has a pile of discrete coins, and paying someone means choosing
//! which ones to destroy and what to give yourself back.
//!
//! The choice is not free. Every coin selected adds ~68 vbytes of fee, and
//! every change output creates a coin you will pay to spend later. Selection
//! is an optimisation problem with a privacy dimension attached: the change
//! amount tells a chain observer roughly what you own.
//!
//! # Two strategies, in order
//!
//! 1. **Branch and bound** looks for a subset that pays the target *without
//!    change at all*. Changeless transactions are smaller, cheaper, leave no
//!    change coin to manage, and give an observer nothing to infer.
//! 2. **Largest first** runs when no such subset exists, which is most of the
//!    time. It produces change.
//!
//! This is the shape Bitcoin Core uses. One deliberate difference: Core
//! shuffles its candidates, and this does not. ARCHITECTURE.md §3 requires the
//! builder to be deterministic — the same coins and the same intent must give
//! the same transaction every time, or the review step in §4 is reviewing
//! something that might not be what gets signed.

use bitcoin::{Amount, FeeRate};

use crate::fee::input_fee;
use crate::utxo::Utxo;

/// A coin under consideration, with the cost of spending it already subtracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Position in the caller's original list.
    pub index: usize,
    /// Face value of the coin.
    pub value: Amount,
    /// Face value minus what this input costs in fees at the chosen rate.
    ///
    /// This is what the coin is worth to *this* transaction. It can be
    /// negative, in which case the coin is not worth spending and is dropped
    /// before selection begins.
    pub effective_value: Amount,
}

/// Why no selection could be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Every coin costs more to spend than it is worth at this fee rate.
    ///
    /// Not the same as having no coins. The wallet may show a balance and
    /// still be unable to spend any of it, which is a state a user needs
    /// explained rather than reported as "insufficient funds".
    AllCoinsUneconomic {
        /// How many coins were considered.
        considered: usize,
        /// Their total face value.
        total_value: Amount,
    },
    /// The economically spendable coins do not cover the target.
    InsufficientFunds {
        /// What was needed, including fees.
        needed: Amount,
        /// What was available after dropping uneconomic coins.
        available: Amount,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::AllCoinsUneconomic {
                considered,
                total_value,
            } => write!(
                f,
                "all {considered} coins ({total_value}) cost more to spend than they are worth at this fee rate"
            ),
            Error::InsufficientFunds { needed, available } => {
                write!(f, "need {needed} including fees, only {available} is spendable")
            }
        }
    }
}

impl std::error::Error for Error {}

/// How the coins were chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Branch and bound found a changeless solution.
    BranchAndBound,
    /// No changeless solution existed; change will be produced.
    LargestFirst,
}

/// The outcome of selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Indices into the caller's coin list.
    pub indices: Vec<usize>,
    /// Which strategy produced this.
    pub strategy: Strategy,
}

/// Maximum nodes branch-and-bound will visit before giving up.
///
/// The search space is exponential. Bitcoin Core uses the same bound. Hitting
/// it is not an error — it means "no changeless solution found cheaply", and
/// the fallback takes over.
const MAX_TRIES: usize = 100_000;

/// Chooses coins to fund `target`.
///
/// `target` is the recipient amount plus the fee for everything that is not an
/// input — the transaction overhead and the outputs. Input fees are already
/// subtracted from each candidate's effective value, which is what makes the
/// arithmetic below a plain subset-sum rather than a moving target.
pub fn select(utxos: &[Utxo], target: Amount, rate: FeeRate) -> Result<Selection, Error> {
    let per_input = input_fee(rate);

    let mut candidates: Vec<Candidate> = utxos
        .iter()
        .enumerate()
        .filter_map(|(index, utxo)| {
            utxo.value.checked_sub(per_input).map(|effective_value| Candidate {
                index,
                value: utxo.value,
                effective_value,
            })
        })
        .filter(|candidate| candidate.effective_value > Amount::ZERO)
        .collect();

    if candidates.is_empty() {
        return Err(Error::AllCoinsUneconomic {
            considered: utxos.len(),
            total_value: utxos.iter().map(|u| u.value).sum(),
        });
    }

    // Descending by effective value. Branch and bound needs a fixed order to be
    // deterministic, and descending is what makes its pruning effective: the
    // largest remaining coin bounds what the rest of the branch can reach.
    candidates.sort_by(|a, b| {
        b.effective_value
            .cmp(&a.effective_value)
            .then(a.index.cmp(&b.index))
    });

    let available: Amount = candidates.iter().map(|c| c.effective_value).sum();
    if available < target {
        return Err(Error::InsufficientFunds {
            needed: target,
            available,
        });
    }

    if let Some(indices) = branch_and_bound(&candidates, target, crate::fee::cost_of_change(rate)) {
        return Ok(Selection {
            indices,
            strategy: Strategy::BranchAndBound,
        });
    }

    Ok(Selection {
        indices: largest_first(&candidates, target),
        strategy: Strategy::LargestFirst,
    })
}

/// Searches for a subset whose effective values land in
/// `[target, target + cost_of_change]`.
///
/// Anything in that window is better than making change: the overshoot is
/// smaller than what a change output would cost to create and later spend, so
/// paying it to the miner is the cheaper outcome.
///
/// Depth-first with three pruning rules, over a fixed descending order.
fn branch_and_bound(
    candidates: &[Candidate],
    target: Amount,
    cost_of_change: Amount,
) -> Option<Vec<usize>> {
    let upper_bound = target + cost_of_change;

    // Suffix sums: the most any branch can still add from position `i` onward.
    // Without this the search cannot prune and degenerates into brute force.
    let mut remaining_from = vec![Amount::ZERO; candidates.len() + 1];
    for position in (0..candidates.len()).rev() {
        remaining_from[position] =
            remaining_from[position + 1] + candidates[position].effective_value;
    }

    // Explicit stack rather than recursion: the depth is the number of coins,
    // which is caller-controlled and must not be able to blow the real one.
    struct Frame {
        position: usize,
        selected: Vec<usize>,
        total: Amount,
    }

    let mut stack = vec![Frame {
        position: 0,
        selected: Vec::new(),
        total: Amount::ZERO,
    }];
    let mut tries = 0usize;

    while let Some(frame) = stack.pop() {
        tries += 1;
        if tries > MAX_TRIES {
            return None;
        }

        // Overshot the window — this branch and everything below it is worse.
        if frame.total > upper_bound {
            continue;
        }

        // Inside the window. Descending order means the first solution found is
        // the one using the fewest, largest coins, which is also the smallest
        // transaction.
        if frame.total >= target {
            return Some(frame.selected.iter().map(|&p| candidates[p].index).collect());
        }

        if frame.position >= candidates.len() {
            continue;
        }

        // Even taking everything left cannot reach the target.
        if frame.total + remaining_from[frame.position] < target {
            continue;
        }

        // Pushed in reverse so "include this coin" is explored first.
        stack.push(Frame {
            position: frame.position + 1,
            selected: frame.selected.clone(),
            total: frame.total,
        });

        let mut with_coin = frame.selected.clone();
        with_coin.push(frame.position);
        stack.push(Frame {
            position: frame.position + 1,
            selected: with_coin,
            total: frame.total + candidates[frame.position].effective_value,
        });
    }

    None
}

/// Takes the largest coins until the target is covered.
///
/// Always succeeds when the caller has already checked that the total is
/// sufficient. Produces change, and tends to consolidate a wallet into fewer,
/// larger coins over time — which keeps future transactions small, at the cost
/// of telling an observer more about what is held.
fn largest_first(candidates: &[Candidate], target: Amount) -> Vec<usize> {
    let mut total = Amount::ZERO;
    let mut chosen = Vec::new();

    for candidate in candidates {
        chosen.push(candidate.index);
        total += candidate.effective_value;
        if total >= target {
            break;
        }
    }

    chosen
}
