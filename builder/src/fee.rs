//! Sizes and fees.
//!
//! Bitcoin charges by transaction *size*, not by amount. A payment of one
//! satoshi from twelve inputs costs more than a payment of a million from one.
//! Everything in this module exists because of that.
//!
//! None of the sizes are computed by hand. `rust-bitcoin`'s weight prediction
//! is used instead, so the numbers stay correct if the rules change and there
//! is no arithmetic here to get subtly wrong.

use bitcoin::hashes::Hash;
use bitcoin::transaction::{predict_weight, InputWeightPrediction};
use bitcoin::{Amount, FeeRate, Weight};

/// The output script length of a P2WPKH output: OP_0 PUSH20 <20 bytes>.
pub const P2WPKH_SCRIPT_LEN: usize = 22;

/// The prediction used for every input.
///
/// Thadam only derives P2WPKH addresses (§5, BIP-84), so every coin it can
/// spend is P2WPKH. `_MAX` assumes a worst-case 72-byte signature; real
/// signatures are usually 71 after low-R grinding. That makes the estimate
/// slightly conservative, which is the correct direction: overpaying by a
/// satoshi per input is invisible, while underpaying leaves a transaction
/// stuck in the mempool with no way to cancel it.
const INPUT_PREDICTION: InputWeightPrediction = InputWeightPrediction::P2WPKH_MAX;

/// Predicted virtual size of a transaction with the given shape.
pub fn predicted_vsize(input_count: usize, output_count: usize) -> u64 {
    weight(input_count, output_count).to_vbytes_ceil()
}

fn weight(input_count: usize, output_count: usize) -> Weight {
    predict_weight(
        std::iter::repeat_n(INPUT_PREDICTION, input_count),
        std::iter::repeat_n(P2WPKH_SCRIPT_LEN, output_count),
    )
}

/// The vsize one additional input adds.
///
/// Derived by difference rather than written down, so it cannot drift from
/// what `predicted_vsize` actually charges.
pub fn vsize_per_input() -> u64 {
    predicted_vsize(2, 1) - predicted_vsize(1, 1)
}

/// The vsize one additional output adds.
pub fn vsize_per_output() -> u64 {
    predicted_vsize(1, 2) - predicted_vsize(1, 1)
}

/// Fee for a given vsize at a given rate, rounded up.
pub fn fee_for_vsize(rate: FeeRate, vsize: u64) -> Amount {
    rate.fee_vb(vsize).unwrap_or(Amount::MAX)
}

/// What one input costs to spend at this fee rate.
///
/// Subtracted from a coin's face value to get its *effective* value — what it
/// is actually worth to this transaction. A 300-satoshi coin at a high fee rate
/// can be worth less than nothing, and spending it makes the user poorer.
pub fn input_fee(rate: FeeRate) -> Amount {
    fee_for_vsize(rate, vsize_per_input())
}

/// The full cost of producing change rather than avoiding it.
///
/// Two fees, not one: the bytes to create the change output now, and the bytes
/// to spend it in some future transaction. Both are real, and counting only the
/// first is why naive wallets accumulate dust they can never economically
/// spend.
///
/// This is the width of the window branch-and-bound searches in. A solution
/// that overshoots the target by less than this is cheaper than making change.
pub fn cost_of_change(rate: FeeRate) -> Amount {
    fee_for_vsize(rate, vsize_per_output()) + input_fee(rate)
}

/// The smallest amount a P2WPKH output may carry and still be relayed.
///
/// Below this, the network treats the output as dust and refuses the
/// transaction. Currently 294 satoshis, taken from the library rather than
/// written down here.
pub fn dust_threshold() -> Amount {
    bitcoin::ScriptBuf::new_p2wpkh(&bitcoin::WPubkeyHash::from_byte_array([0u8; 20]))
        .minimal_non_dust()
}
