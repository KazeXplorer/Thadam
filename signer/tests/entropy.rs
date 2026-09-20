//! Tests for `generate_entropy`.
//!
//! The OS entropy source cannot be seeded or controlled, so these tests check
//! shape, size and statistical properties rather than exact values.

use thadam_signer::{generate_entropy, mnemonic_from_entropy};

/// BIP-39 permits exactly three entropy sizes. Each maps to a distinct byte
/// count; anything else is an error.
#[test]
fn valid_bit_sizes_produce_the_right_byte_counts() {
    let cases = [(128u32, 16usize), (192, 24), (256, 32)];
    for (bits, expected_bytes) in cases {
        let entropy = generate_entropy(bits)
            .unwrap_or_else(|e| panic!("generate_entropy({bits}) failed: {e}"));
        assert_eq!(
            entropy.len(),
            expected_bytes,
            "bits={bits} should give {expected_bytes} bytes"
        );
    }
}

/// Sizes outside the BIP-39 set must be rejected rather than silently
/// producing broken entropy.
#[test]
fn invalid_bit_sizes_are_rejected() {
    for bad in [0u32, 64, 127, 129, 160, 191, 193, 248, 257, 512] {
        assert!(
            generate_entropy(bad).is_err(),
            "generate_entropy({bad}) should have failed but succeeded"
        );
    }
}

/// Entropy from the OS CSPRNG must not be all-zero. An all-zero block is
/// technically possible but astronomically unlikely; if it happens here the
/// entropy source is broken.
#[test]
fn output_is_not_all_zero() {
    for bits in [128u32, 192, 256] {
        let entropy = generate_entropy(bits).unwrap();
        assert!(
            entropy.iter().any(|&b| b != 0),
            "generate_entropy({bits}) returned all-zero bytes — entropy source is broken"
        );
    }
}

/// Two consecutive calls must produce different bytes.
///
/// Repeated output would indicate a broken or mocked RNG. Like the all-zero
/// check this is astronomically unlikely to false-positive on a real source.
#[test]
fn two_calls_produce_different_output() {
    for bits in [128u32, 192, 256] {
        let a = generate_entropy(bits).unwrap();
        let b = generate_entropy(bits).unwrap();
        assert_ne!(
            a, b,
            "generate_entropy({bits}) returned identical bytes twice — RNG is not random"
        );
    }
}

/// The entropy flows all the way through to a valid mnemonic.
///
/// This is the integration path the mobile layer will use: generate → phrase
/// → derive. If any step rejects the bytes the user can never create a wallet.
#[test]
fn generated_entropy_produces_a_valid_mnemonic() {
    for bits in [128u32, 192, 256] {
        let entropy = generate_entropy(bits).unwrap();
        mnemonic_from_entropy(&entropy).unwrap_or_else(|e| {
            panic!("mnemonic_from_entropy rejected entropy from generate_entropy({bits}): {e}")
        });
    }
}

/// Word counts match entropy sizes: 128 → 12, 192 → 18, 256 → 24.
///
/// This pins the BIP-39 relationship so a future refactor that changed the
/// byte count would immediately break here rather than silently producing
/// wallets with the wrong number of words.
#[test]
fn mnemonic_word_count_matches_entropy_size() {
    let cases = [(128u32, 12usize), (192, 18), (256, 24)];
    for (bits, expected_words) in cases {
        let entropy = generate_entropy(bits).unwrap();
        let mnemonic = mnemonic_from_entropy(&entropy).unwrap();
        let word_count = mnemonic.word_count();
        assert_eq!(
            word_count, expected_words,
            "bits={bits} should produce {expected_words} words, got {word_count}"
        );
    }
}
