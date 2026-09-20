# Language Workflow — Signer Patterns

Continues from `01-language-workflow.md`. Same approach: why the code is
shaped this way, not what each keyword means. Anchored to `signer/src/`.

---

## 1. The Newtype Pattern — `Address` Wrapping a String

```rust
pub struct Address {
    address: String,
    network: BitcoinNetwork,
}
```

A raw `String` can hold any address. It doesn't know which network it belongs
to. The `Address` struct is a "newtype" — it wraps a `String` but adds a
guarantee: you can't have an `Address` without a `network`.

**What this buys you:** Functions that take `Address` instead of `&str` can
never receive an address without a network. The type system enforces the rule
"an address always knows which network it's for" at every call site, not just
at the one place that creates it.

**The alternative:** Pass `(String, BitcoinNetwork)` as a tuple everywhere.
Works, but nothing stops someone from passing a testnet4 address with
`BitcoinNetwork::Regtest`. The newtype controls construction — the only ways
to create an `Address` are `p2wpkh()` and `parse()`, both of which validate.

**When you'd skip it:** If the data has no invariant to protect. A display
name like "Bitcoin testnet4" is just a string — wrapping it in a struct adds
ceremony without safety. Newtypes earn their weight when construction must
be validated.

---

## 2. Rich Error Enums — Structured Failure

```rust
pub enum Error {
    InvalidMnemonic(String),
    Derivation(String),
    MalformedAddress,
    WrongNetwork { expected: BitcoinNetwork, address: String },
    HrpMismatch { expected: String, produced: String },
}
```

Each variant carries the data needed to explain what went wrong. `WrongNetwork`
carries both the expected network AND the address, so a UI can say "that is a
regtest address and you are on testnet4" instead of just "invalid address."

**Why an enum instead of a `String` error:**
- **Matching:** The caller can `match` on the variant and handle each failure
  differently. `WrongNetwork` → show a helpful message. `Derivation` → log
  and crash (shouldn't happen in normal use).
- **Exhaustiveness:** Add a new error variant and every `match` that doesn't
  handle it stops compiling. With string errors, a new failure mode is invisible
  to existing code.
- **No exception hierarchy:** Rust has no exceptions. Errors are values you
  return, not things you throw. This means every function's signature tells
  you exactly what can go wrong — `Result<Address, Error>` means "this gives
  you an address or one of these five specific problems."

**The `.map_err()` chain pattern:**
```rust
let master = Xpriv::new_master(to_library_network(network), &seed)
    .map_err(|e| Error::Derivation(e.to_string()))?;
```

The library returns its own error type. `.map_err()` converts it to Thadam's
error type. The `?` at the end says "if this is an error, return it from this
function immediately." This is Rust's replacement for try/catch — errors
propagate explicitly, and every conversion is visible in the code.

**The TypeScript equivalent:** You'd use a discriminated union:
```typescript
type ParseResult =
  | { ok: true; id: EnvironmentId }
  | { ok: false; error: ParseEnvironmentIdError };
```
Same idea — structured failure, the caller matches on the shape. Your
`ui-catalog/src/id.ts` already does this.

---

## 3. Module as Access Control — The Chokepoint

```rust
// network.rs
pub(crate) fn to_library_network(network: BitcoinNetwork) -> Network { ... }
```

`pub(crate)` means "visible inside this crate but not outside it." No code
that depends on `thadam-signer` can call `to_library_network` directly or
even name `bitcoin::Network`.

**What this enforces:** The module comment spells it out — "nothing outside
this module may name `bitcoin::Network`." The library knows about mainnet.
Thadam doesn't want to. So there's exactly ONE function that crosses the
boundary, it's an exhaustive match with no mainnet arm, and it's `pub(crate)`
so nobody outside the signer can bypass it.

**This is a code architecture pattern, not a Rust feature.** The Rust feature
is visibility modifiers. The pattern is: when a dependency has more power than
you want your code to have, wrap the transition in a single chokepoint function
and make it the only way through.

**In other languages:**
- Java/Kotlin: `internal` or package-private
- TypeScript: no built-in module-private, but you'd use a barrel export
  (`index.ts`) that doesn't re-export the function
- Go: unexported (lowercase) function in the package

---

## 4. Don't Re-export Dependency Types

```rust
pub use bip39::Mnemonic;  // Re-exported — callers use it directly
```

But notice what ISN'T re-exported: `bitcoin::Network`, `bitcoin::bip32::Xpriv`,
`secp256k1::Secp256k1`. Those are implementation details. The public API is:

```
parse_mnemonic(phrase) → Mnemonic
mnemonic_from_entropy(bytes) → Mnemonic
receive_address(mnemonic, passphrase, network, ...) → Address
bip84_path(network, ...) → DerivationPath
```

**Why `Mnemonic` is re-exported but `Xpriv` is not:** `Mnemonic` appears in
the public API — the caller creates one and passes it to `receive_address`.
If it weren't re-exported, the caller would need to add `bip39` to their own
dependencies just to name the type. `Xpriv` is used only inside
`receive_address` — the caller never sees it.

**The rule:** Re-export a dependency type when your callers need to name it.
Don't when they don't. Every re-exported type becomes a commitment — if you
switch from the `bip39` crate to another one, every caller's code breaks.
The fewer types you expose, the easier it is to swap internals later.

---

## 5. Tests as Regression Traps, Not Coverage Checklists

The signer's test file (`tests/wiring.rs`) has a specific philosophy stated
in its header: "the tests that existed before tested the library against
itself — green forever, proving nothing."

**What changed:** When the signer hand-rolled BIP-39 and BIP-32, the tests
compared its output to `rust-bitcoin`. Now that `rust-bitcoin` IS the
implementation, those tests would be `f(x) == f(x)` — tautologically true.

**What replaced them:** Tests that check the WIRING — the part that's actually
Thadam's code:

| Test | What it actually catches |
|---|---|
| `the_standard_phrase_derives_pinned_addresses` | If you change the BIP-84 purpose, the coin type, or the address type, this breaks. Those are wallet-destroying changes. |
| `passphrases_change_the_wallet_and_normalise_consistently` | Regression for the NFKD bug. If someone switches from `to_seed()` to `to_seed_normalized()`, this catches it. |
| `bip84_paths_use_the_registrys_coin_type` | The path is built from the registry, not a hardcoded `1`. |
| `testnet4_and_signet_addresses_are_indistinguishable` | Documents a known hazard as a passing test. |

**The pattern:** Every test answers "what specific mistake would this catch?"
If the answer is "none, it just exercises the code," the test is noise. This
is the difference between testing for correctness (does it work?) and testing
for wiring (is it connected to the right things?).

---

## 6. Why the Comment About `to_seed` vs `to_seed_normalized`

```rust
// `to_seed`, not `to_seed_normalized`. The latter means "the passphrase is
// ALREADY NFKD-normalised" and skips the step — which reintroduces exactly
// the bug the hand-written version had, now with a library's name on it.
```

This is a pattern worth recognising: **a library offering two functions that
differ by one assumption.** One does the work, one assumes it's already done.
Pick the wrong one and your code silently produces different output from every
other wallet — no error, no crash, just wrong keys.

**Where you'll see this again:**
- Hashing libraries: `hash(data)` vs `hash_raw(pre-padded_data)`
- Crypto libraries: `sign(message)` vs `sign_prehash(hash)` — one hashes for
  you, the other assumes you already did
- Encoding libraries: `encode(string)` vs `encode_bytes(already_utf8_bytes)`

The defence is exactly what your code does: a comment explaining WHY this
variant was chosen, and a test that breaks if someone swaps it.

---

## Summary — Patterns Added by the Signer

| Pattern | What it does | The signer's example |
|---|---|---|
| Newtype | Bundles data + invariant so the type system enforces rules | `Address` = string + network, no way to separate them |
| Rich error enum | Makes failure structured and matchable | `Error::WrongNetwork` carries both sides for a good UI message |
| `.map_err()` + `?` | Explicit error propagation, every conversion visible | Converting library errors to Thadam errors one line at a time |
| `pub(crate)` chokepoint | One function bridges to a more-powerful dependency | `to_library_network()` is the only place `bitcoin::Network` is named |
| Selective re-export | Expose only what callers need to name | `Mnemonic` is public, `Xpriv` is not |
| Tests as wiring checks | Test YOUR code, not the library's | Pinned addresses catch path assembly bugs, not crypto bugs |
| "Which variant" comments | Document why this API call, not the other one | `to_seed` not `to_seed_normalized` |
