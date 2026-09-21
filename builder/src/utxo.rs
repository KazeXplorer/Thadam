//! The coins a transaction can spend.

use bitcoin::{Amount, OutPoint};
use thadam_signer::Address;

/// One unspent output the wallet controls.
///
/// The address carries its network (ARCHITECTURE.md §8), so a set of coins
/// cannot silently mix networks — [`crate::build`] rejects that rather than
/// producing a transaction that could never confirm anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utxo {
    /// Which output of which transaction this is.
    pub outpoint: OutPoint,
    /// What it is worth, before any fee is considered.
    pub value: Amount,
    /// The address it pays to — one of ours, or we could not spend it.
    pub address: Address,
}

/// What the user asked for.
///
/// Deliberately one recipient. Batching is a real feature and a different
/// shape, and §10 says no structure exists before something goes in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    /// Who is being paid.
    pub recipient: Address,
    /// How much they receive. This is exact — fees come out of the change, not
    /// out of the recipient's amount.
    pub amount: Amount,
    /// Where change goes, when there is any.
    ///
    /// Required even when the transaction turns out to be changeless, because
    /// whether it is changeless is not known until selection has run, and a
    /// builder that could not place change would have to fail late.
    pub change_address: Address,
    /// What the user is willing to pay per virtual byte.
    pub fee_rate: bitcoin::FeeRate,
}
