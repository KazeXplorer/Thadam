# Chain Internals — How Keys, Addresses, and Signing Work Behind the Scenes

Anchored to Thadam's signer (`signer/src/`). Every concept below maps to a
line of code you already have. This doc walks the pipeline your code
implements — what happens at each step, why it works that way, and what
would go wrong if it didn't.

---

## The Full Pipeline in One Picture

```
Entropy (16-32 random bytes)
    |
    v
BIP-39: entropy → mnemonic words → seed (512-bit)
    |
    v
BIP-32: seed → master private key + chain code (xprv / tprv)
    |
    v
BIP-84 path: m/84'/coin_type'/account'/change/index
    |
    v
Derived private key
    |
    v
secp256k1: private key → compressed public key (33 bytes)
    |
    v
Hash: SHA-256 → RIPEMD-160 → 20-byte pubkey hash
    |
    v
bech32: pubkey hash + HRP + witness version → tb1q... address
```

Your `receive_address()` function in `lib.rs` IS this pipeline. Every step
below corresponds to a line or call in it.

---

## Step 1: Entropy — Where Randomness Comes From

**Your code:** `mnemonic_from_entropy(entropy: &[u8])`

**What's happening:** A wallet starts with random bytes. 16 bytes = 12 words,
32 bytes = 24 words. The quality of this randomness is the entire security
foundation — if the entropy is predictable, every key derived from it is
predictable.

**Why your code takes entropy as an argument rather than generating it:**
The doc comment says it explicitly — "where randomness comes from is a decision
that belongs at the edge of the system." In a browser (WASM), you'd use
`crypto.getRandomValues()`. On a server, `/dev/urandom`. In a test, a fixed
seed. The signer doesn't pick — the caller does. This is a deliberate design
boundary.

**What goes wrong if entropy is bad:** In 2014, an Android Bitcoin wallet had
a bug in its random number generator. The same `k` value was reused in two
ECDSA signatures, which leaked the private key. Anyone watching the blockchain
could compute the key and steal the funds. Bad entropy doesn't produce an
error — it produces keys that look fine but are guessable.

---

## Step 2: BIP-39 — Mnemonic Words

**Your code:** `parse_mnemonic(phrase)` / `Mnemonic::parse_normalized(phrase)`

**What BIP-39 does:**
1. Takes entropy (e.g. 16 bytes)
2. Computes a checksum (first few bits of SHA-256 of the entropy)
3. Appends the checksum bits to the entropy
4. Splits the result into 11-bit chunks
5. Each chunk indexes into a fixed wordlist of 2048 words

So "abandon abandon ... about" isn't random English — each word encodes
exactly 11 bits of the original entropy + checksum.

**Why words instead of raw hex:** Humans can't write down 32 hex bytes
reliably. They can write 12 English words on paper and store it in a safe.
The words ARE the entropy — there's no separate "key file." Lose the words,
lose the wallet. Type them into any BIP-39 compatible wallet, recover
everything.

**The NFKD bug your code documents:** BIP-39 specifies that the passphrase
must be normalised using Unicode NFKD before hashing. The character `é` can
be represented two ways in Unicode — as a single codepoint (`U+00E9`) or as
`e` + combining acute accent (`U+0065 U+0301`). Without NFKD normalisation,
these produce different seeds, which means different wallets. Your hand-written
version missed this. The `bip39` crate handles it. Your test
`passphrases_change_the_wallet_and_normalise_consistently` pins it.

**Mnemonic → Seed:** `mnemonic.to_seed(passphrase)` runs PBKDF2-HMAC-SHA512
with 2048 rounds. Input: the mnemonic string + "mnemonic" + passphrase as
salt. Output: 512 bits (64 bytes). This is intentionally slow — 2048 rounds
of hashing makes brute-forcing harder.

---

## Step 3: BIP-32 — Hierarchical Deterministic Key Derivation

**Your code:** `Xpriv::new_master(network, &seed)` then `master.derive_priv(&secp, &path)`

**What "hierarchical deterministic" means:** From one seed, derive an
unlimited tree of key pairs. Any path down the tree gives a different key.
Same seed + same path = same key, always, on any machine, in any wallet that
implements BIP-32. This is how recovery works — you don't back up keys, you
back up the seed (the 12 words), and every key can be recomputed.

**The master key:** `Xpriv::new_master` runs HMAC-SHA512 on the seed with
the key "Bitcoin seed". The left 256 bits become the master private key. The
right 256 bits become the "chain code" — extra entropy that makes derivation
non-reversible. Together they form the extended private key (`tprv...` on
testnet, `xprv...` on mainnet — those prefixes come from `bip32_private_version`
in your registry).

**Child derivation — what `derive_priv` does at each step:**
Each step down the path takes a parent (key + chain code) and produces a
child (key + chain code):

1. For **hardened** children (index ≥ 2³¹, written with `'`):
   Input to HMAC-SHA512 = `0x00 || parent_private_key || index`
   Key = "the parent chain code"

2. For **normal** children (index < 2³¹):
   Input to HMAC-SHA512 = `parent_public_key || index`
   Key = "the parent chain code"

Left 256 bits of output + parent private key (mod curve order) = child
private key. Right 256 bits = child chain code.

**Why hardened vs normal matters:**
- **Hardened (`'`):** Requires the parent private key. Knowing the parent
  PUBLIC key + child chain code does NOT let you compute the child private key.
  Used for purpose, coin_type, and account — the levels where leaking a child
  should not compromise the parent.
- **Normal:** Uses the parent public key. This means a watch-only wallet with
  only the parent extended PUBLIC key can derive all child PUBLIC keys and
  generate addresses without ever seeing a private key. Used for change and
  index — so a watch-only wallet can enumerate addresses.

**Your BIP-84 path:** `m/84'/1'/account'/change/index`
- `84'` — hardened, means "native SegWit (P2WPKH) addresses" (BIP-84)
- `1'` — hardened, coin type from registry (testnet = 1, mainnet = 0)
- `account'` — hardened, which "account" within this wallet
- `change` — normal, 0 = receive address, 1 = change address
- `index` — normal, which address within this account

---

## Step 4: From Private Key to Public Key (secp256k1)

**Your code:** `CompressedPublicKey::from_private_key(&secp, &private_key)`

**What secp256k1 is:** An elliptic curve. A private key is a 256-bit integer
(a very large number). A public key is a point on this curve, computed by
multiplying the curve's "generator point" G by the private key:

```
public_key = private_key × G
```

This multiplication is a one-way function: going from private → public is
fast (a few milliseconds). Going from public → private is computationally
infeasible — it would take longer than the age of the universe with current
hardware.

**Why "compressed":** A point on the curve has two coordinates (x, y).
Both are 256 bits, so an uncompressed public key is 65 bytes (1 prefix byte +
32 for x + 32 for y). But given x, there are only two possible y values
(the curve equation is symmetric). So a compressed key is 33 bytes: 1 prefix
byte (0x02 if y is even, 0x03 if y is odd) + 32 bytes for x. The other wallet
can recompute y. Every modern Bitcoin address uses compressed keys.

**Why this step is where security lives:** The elliptic curve discrete
logarithm problem (ECDLP) is what makes Bitcoin work. If someone could
efficiently solve it, they could compute every private key from its public key
and steal every coin on the network. Current consensus: no classical computer
can. Quantum computers are a theoretical threat but not a practical one at
current scale.

---

## Step 5: Public Key → Address (Hashing + bech32)

**Your code:** `bitcoin::Address::p2wpkh(public_key, hrp)` — the library does
this in one call, but inside it:

### 5a. Hashing the public key

```
compressed_pubkey (33 bytes)
    → SHA-256 → 32 bytes
    → RIPEMD-160 → 20 bytes = "pubkey hash"
```

**Why hash at all? Why not use the public key directly as the address?**
Two reasons:
1. **Size** — 20 bytes is shorter than 33 bytes. Addresses go into
   transactions, which go into blocks, which are size-limited.
2. **Quantum resistance (partial)** — Until you SPEND from an address, only
   the hash is on the blockchain. The actual public key isn't revealed. A
   quantum computer that could break secp256k1 still can't reverse the hash.
   Once you spend (revealing the public key in the transaction), this
   protection is gone — which is why "don't reuse addresses" is standard
   advice.

**Why SHA-256 then RIPEMD-160?** Historical. Satoshi chose it. Using two
different hash functions means an attacker needs to break BOTH to find a
collision. RIPEMD-160's 160-bit output was chosen for shorter addresses.

### 5b. bech32 encoding

```
witness_version (0 for P2WPKH) + pubkey_hash (20 bytes) + HRP ("tb")
    → bech32 encoding
    → "tb1q6rz28mcfaxtmd6v789l9rrlrusdprr9pqcpvkl"
```

**What bech32 is:** A human-readable encoding scheme (like base64, but
designed for addresses). It uses only lowercase alphanumeric characters
(no `1`, `b`, `i`, `o` to avoid visual confusion) and includes a checksum
that detects up to 4 errors and locates up to 1.

**Breaking down the address string:**
- `tb` — the HRP from your registry (`bech32_hrp`). Says "this is testnet."
- `1` — the separator (always `1`)
- `q` — encodes witness version 0 (SegWit v0, P2WPKH). `p` would be version
  1 (Taproot/P2TR).
- `6rz28mcfaxtmd6v789l9rrlrusdprr9p` — the pubkey hash, bech32-encoded
- `qcpvkl` — the checksum (6 characters)

**Your `Address` struct carries `network` alongside the string because:**
`tb1q6rz28mcfaxtmd6v789l9rrlrusdprr9pqcpvkl` is valid on testnet4 AND signet.
The string alone is ambiguous. Your test
`testnet4_and_signet_addresses_are_indistinguishable` documents this as a
passing assertion.

---

## Step 6: What "Signing" Will Do (Not Built Yet)

Your `lib.rs` says signing comes later. Here's what the pipeline will look like
when it arrives, so you recognise the shape:

### What a transaction is

A Bitcoin transaction says: "take coins from these inputs (previous
transactions' outputs) and create these new outputs (amounts + locking
conditions)."

```
Transaction {
    inputs: [
        { prev_txid, prev_output_index, script_sig / witness }
    ],
    outputs: [
        { amount, script_pubkey }
    ]
}
```

The `script_pubkey` of a P2WPKH output is: `OP_0 <20-byte-pubkey-hash>`.
That hash is the same one in the address. Sending to `tb1q6rz...` means
creating an output whose `script_pubkey` contains that pubkey hash.

### What signing does

To SPEND an output (use it as an input in a new transaction), you prove you
own the private key whose public key hashes to the pubkey hash locked in that
output:

1. **Serialise** the transaction (with special rules for which parts are
   included — "sighash")
2. **Hash** the serialised transaction (double SHA-256)
3. **Sign** that hash with the private key using ECDSA (or Schnorr for
   Taproot)
4. Put the signature + public key in the input's `witness` field

A node verifying the transaction:
1. Takes the public key from the witness
2. Hashes it → checks it matches the pubkey hash in the output being spent
3. Verifies the signature against the transaction hash using the public key

If both check out, the spend is valid. The private key is never transmitted —
only the signature, which proves possession without revealing the key.

### What your signer will need from the registry

When signing arrives, the signer will need:
- The derived private key (already built — `derive_priv`)
- The transaction to sign (will come from a transaction builder)
- The network params for the sighash (which your registry holds)

The registry constants don't change for signing — they're the same ones used
for address derivation. The signer just uses them in a different context.

---

## The Security Model in One Sentence

The entire chain of BIP-39 → BIP-32 → secp256k1 → hash → bech32 is
designed so that **knowing any output doesn't let you reverse to any earlier
step**: an address doesn't reveal the public key, a public key doesn't reveal
the private key, a private key doesn't reveal the seed, and a seed doesn't
reveal the entropy. Each step is a one-way function. Signing proves you
went through the whole chain without showing anyone else how to repeat it.

---

## Mapping Back to Your Code

| Pipeline step | Your code | Registry field used |
|---|---|---|
| Entropy → mnemonic | `mnemonic_from_entropy()` | none |
| Parse mnemonic | `parse_mnemonic()` | none |
| Mnemonic → seed | `mnemonic.to_seed(passphrase)` | none |
| Seed → master key | `Xpriv::new_master(network, &seed)` | `bip32_private_version` (via library) |
| Build derivation path | `bip84_path(network, ...)` | `bip44_coin_type` |
| Derive child key | `master.derive_priv(&secp, &path)` | none (pure math) |
| Private → public key | `CompressedPublicKey::from_private_key()` | none (secp256k1) |
| Public key → address | `Address::p2wpkh(&pubkey, network)` | `bech32_hrp` (checked, not used to build) |
| Network chokepoint | `to_library_network()` | all (implicitly — enum has no mainnet) |

The "none" entries are pure cryptographic operations — they work the same
regardless of which network you're on. The registry enters the picture at the
boundaries: when you need to say WHICH network's key tree to derive from, and
when you need to say WHICH network's address format to encode into.
