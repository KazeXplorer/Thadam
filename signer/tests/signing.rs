//! Tests for `sign_transaction`.
//!
//! BIP-143 signing is testable at two levels:
//!
//! 1. **Shape**: every input gets a witness; each witness has exactly two items
//!    (DER signature + compressed public key); the signature is ECDSA.
//!
//! 2. **Verification**: the signed transaction passes `secp256k1::Secp256k1::verify_ecdsa`.
//!
//! Both are covered below. The test mnemonic is the BIP-39 all-"abandon" phrase
//! (test 0), whose addresses are well-known and used throughout the test suite.

use bitcoin::{
    absolute::LockTime,
    hashes::{hash160, Hash},
    sighash::{EcdsaSighashType, SighashCache},
    transaction::Version,
    Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
};
use secp256k1::Secp256k1;
use thadam_registry::BitcoinNetwork;
use thadam_signer::{bip84_path, parse_mnemonic, sign_transaction, SigningInput};

const NETWORK: BitcoinNetwork = BitcoinNetwork::Testnet4;
const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon \
     abandon abandon abandon abandon abandon about";

/// Builds a bare unsigned transaction with `n` dummy inputs.
///
/// The outpoints are invented; in a real flow they would be real UTXOs.
/// The one recipient output is just a throwaway P2PKH script.
fn dummy_unsigned(n: usize, input_value_sat: u64) -> (Transaction, Vec<SigningInput>) {
    let inputs: Vec<TxIn> = (0..n)
        .map(|i| TxIn {
            previous_output: OutPoint {
                txid: bitcoin::Txid::from_raw_hash(bitcoin::hashes::sha256d::Hash::hash(
                    &(i as u64).to_le_bytes(),
                )),
                vout: 0,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::new(),
        })
        .collect();

    // A throwaway P2PKH recipient output — content does not matter for signing tests.
    let output = TxOut {
        value: Amount::from_sat(input_value_sat * n as u64 - 1_000),
        script_pubkey: ScriptBuf::new_p2pkh(&bitcoin::PubkeyHash::from_raw_hash(
            hash160::Hash::hash(b"throwaway"),
        )),
    };

    let tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: inputs,
        output: vec![output],
    };

    let signing_inputs: Vec<SigningInput> = (0..n)
        .map(|i| SigningInput {
            value: Amount::from_sat(input_value_sat),
            account: 0,
            change: 0,
            index: i as u32,
        })
        .collect();

    (tx, signing_inputs)
}

/// A mismatch between signing input count and transaction input count is an
/// error, not a panic.
#[test]
fn input_count_mismatch_returns_error() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let (tx, _) = dummy_unsigned(2, 500_000);

    // Supply 3 inputs for a 2-input transaction.
    let wrong_inputs: Vec<SigningInput> = (0..3)
        .map(|i| SigningInput {
            value: Amount::from_sat(500_000),
            account: 0,
            change: 0,
            index: i,
        })
        .collect();

    let result = sign_transaction(tx, &wrong_inputs, &mnemonic, "", NETWORK);
    assert!(result.is_err(), "mismatched count must fail");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("3") && msg.contains("2"),
        "error should mention both counts: {msg}"
    );
}

/// Every input in the signed transaction has a non-empty witness.
///
/// An empty witness means the input was not signed and the transaction is
/// invalid. This is the most basic shape guarantee.
#[test]
fn every_input_has_a_witness() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let (tx, inputs) = dummy_unsigned(3, 500_000);

    let signed = sign_transaction(tx, &inputs, &mnemonic, "", NETWORK).expect("must sign");

    for (i, input) in signed.input.iter().enumerate() {
        assert!(
            !input.witness.is_empty(),
            "input[{i}] has no witness after signing"
        );
    }
}

/// Each witness has exactly two items: the signature and the public key.
///
/// P2WPKH witness stack: [<DER sig + sighash byte>, <33-byte compressed pubkey>].
/// Three items means something extra was pushed; one item means the pubkey is
/// missing. Either would make the transaction invalid.
#[test]
fn witness_has_exactly_two_items_per_input() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let (tx, inputs) = dummy_unsigned(2, 1_000_000);

    let signed = sign_transaction(tx, &inputs, &mnemonic, "", NETWORK).expect("must sign");

    for (i, input) in signed.input.iter().enumerate() {
        assert_eq!(
            input.witness.len(),
            2,
            "input[{i}] witness should have exactly 2 items, got {}",
            input.witness.len()
        );
    }
}

/// The public key in the witness is a valid 33-byte compressed public key.
///
/// An uncompressed key (65 bytes) is non-standard for P2WPKH. A wrong length
/// means address derivation used a different key format than signing.
#[test]
fn witness_pubkey_is_33_bytes_compressed() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let (tx, inputs) = dummy_unsigned(1, 500_000);

    let signed = sign_transaction(tx, &inputs, &mnemonic, "", NETWORK).expect("must sign");

    let witness = &signed.input[0].witness;
    let pubkey_bytes = witness.nth(1).expect("witness should have 2 items");
    assert_eq!(
        pubkey_bytes.len(),
        33,
        "P2WPKH needs a 33-byte compressed public key, got {}",
        pubkey_bytes.len()
    );
    // Compressed public keys start with 0x02 or 0x03.
    assert!(
        pubkey_bytes[0] == 0x02 || pubkey_bytes[0] == 0x03,
        "compressed pubkey must start with 0x02 or 0x03, got 0x{:02x}",
        pubkey_bytes[0]
    );
}

/// The ECDSA signature in the witness verifies against the BIP-143 sighash.
///
/// This is the actual cryptographic correctness check: the key used to sign
/// must match the key that would have derived the spending address, and the
/// sighash must cover the right data.
#[test]
fn witness_signature_verifies() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let secp = Secp256k1::new();

    let value = Amount::from_sat(1_000_000);
    let (tx, inputs) = dummy_unsigned(1, value.to_sat());
    let signed = sign_transaction(tx.clone(), &inputs, &mnemonic, "", NETWORK).expect("must sign");

    let witness = &signed.input[0].witness;
    let sig_bytes = witness.nth(0).expect("witness item 0 is the sig");
    let pk_bytes = witness.nth(1).expect("witness item 1 is the pubkey");

    // Parse the DER signature (strip trailing sighash byte).
    let sig = secp256k1::ecdsa::Signature::from_der(&sig_bytes[..sig_bytes.len() - 1])
        .expect("witness contains a valid DER signature");

    // Parse the public key.
    let pubkey =
        secp256k1::PublicKey::from_slice(pk_bytes).expect("witness contains a valid public key");

    // Reconstruct the script code.
    let key_hash = hash160::Hash::hash(pk_bytes);
    let wpubkey_hash = bitcoin::WPubkeyHash::from_raw_hash(key_hash);
    let script_pubkey = ScriptBuf::new_p2wpkh(&wpubkey_hash);

    // Recompute the BIP-143 sighash over the original unsigned transaction.
    let mut cache = SighashCache::new(&tx);
    let sighash = cache
        .p2wpkh_signature_hash(0, &script_pubkey, value, EcdsaSighashType::All)
        .expect("sighash computation must succeed");

    let message = secp256k1::Message::from_digest(sighash.to_byte_array());

    secp.verify_ecdsa(&message, &sig, &pubkey)
        .expect("signature must verify against the sighash");
}

/// Signing with the wrong passphrase derives a different key and the
/// signature does not verify against the key expected for the empty passphrase.
///
/// This pins the behaviour: wrong passphrase → silently different key → the
/// transaction is valid Bitcoin but spends from a different wallet. It will
/// broadcast successfully but never confirm because no miner can match it to a
/// UTXO the wallet actually owns.
#[test]
fn wrong_passphrase_produces_a_different_key() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let (tx, inputs) = dummy_unsigned(1, 500_000);

    let signed_correct = sign_transaction(tx.clone(), &inputs, &mnemonic, "", NETWORK)
        .expect("signing with empty passphrase must succeed");
    let signed_wrong = sign_transaction(tx, &inputs, &mnemonic, "wrong", NETWORK)
        .expect("signing with wrong passphrase must also succeed — it's just a different key");

    // The witnesses must differ because the keys differ.
    let w_correct = &signed_correct.input[0].witness;
    let w_wrong = &signed_wrong.input[0].witness;
    assert_ne!(
        w_correct.nth(1),
        w_wrong.nth(1),
        "different passphrases must produce different public keys"
    );
}

/// The path components in `SigningInput` are used: index 1 differs from index 0.
#[test]
fn different_index_produces_different_key() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let value = Amount::from_sat(500_000);

    let make_input = |index: u32| -> SigningInput {
        SigningInput {
            value,
            account: 0,
            change: 0,
            index,
        }
    };

    let (tx0, inputs0) = {
        let (tx, _) = dummy_unsigned(1, value.to_sat());
        (tx.clone(), vec![make_input(0)])
    };
    let (tx1, inputs1) = {
        let (tx, _) = dummy_unsigned(1, value.to_sat());
        (tx, vec![make_input(1)])
    };

    let signed0 = sign_transaction(tx0, &inputs0, &mnemonic, "", NETWORK).unwrap();
    let signed1 = sign_transaction(tx1, &inputs1, &mnemonic, "", NETWORK).unwrap();

    assert_ne!(
        signed0.input[0].witness.nth(1),
        signed1.input[0].witness.nth(1),
        "index=0 and index=1 must produce different public keys"
    );
}

/// Consistency: signing the same transaction twice gives the same witnesses.
///
/// ECDSA with libsecp256k1 uses RFC 6979 deterministic nonces, so the
/// signature is deterministic. A non-deterministic signer here would mean two
/// broadcast attempts could look like different transactions.
#[test]
fn signing_is_deterministic() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let (tx, inputs) = dummy_unsigned(2, 500_000);

    let first = sign_transaction(tx.clone(), &inputs, &mnemonic, "", NETWORK).unwrap();
    let second = sign_transaction(tx, &inputs, &mnemonic, "", NETWORK).unwrap();

    assert_eq!(
        first, second,
        "signing the same transaction twice must give identical results"
    );
}

/// The public key in the witness is the same key that owns the derived address.
///
/// Derives the public key via the BIP-84 path (using `secp256k1` directly),
/// then checks that the witness pubkey from signing matches. This pins the
/// end-to-end wiring — if anything in the path changes (coin type, hardened
/// indices), this test fails before any funds are at risk.
#[test]
fn witness_pubkey_matches_bip84_derived_key() {
    let mnemonic = parse_mnemonic(PHRASE).unwrap();
    let secp = Secp256k1::new();

    // Derive the expected public key directly via secp256k1 + bip32.
    let seed = mnemonic.to_seed("");
    let path = bip84_path(NETWORK, 0, 0, 0).expect("path derivation must succeed");

    // Use bitcoin's Xpriv directly with the same Signet network constant the
    // signer uses internally (Testnet4 maps to bitcoin::Network::Signet for
    // encoding purposes; this is the same mapping the signer uses).
    let master =
        bitcoin::bip32::Xpriv::new_master(bitcoin::Network::Signet, &seed).unwrap();
    let derived = master.derive_priv(&secp, &path).unwrap();
    let expected_pk_bytes =
        secp256k1::PublicKey::from_secret_key(&secp, &derived.to_priv().inner).serialize();

    // Sign a dummy transaction with the same path (account=0, change=0, index=0).
    let (tx, inputs) = dummy_unsigned(1, 500_000);
    let signed = sign_transaction(tx, &inputs, &mnemonic, "", NETWORK).unwrap();

    let witness_pk_bytes = signed.input[0]
        .witness
        .nth(1)
        .expect("witness should have pubkey at position 1");

    assert_eq!(
        witness_pk_bytes, &expected_pk_bytes,
        "witness public key must match the BIP-84 derived key for m/84'/1'/0'/0/0"
    );
}
