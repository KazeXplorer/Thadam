# Signer

Recovery phrase → keys → address. ARCHITECTURE.md §3: the only layer with keys.

```
signer/
  src/
    lib.rs       the API: phrase → seed → path → address
    network.rs   the mainnet chokepoint — one conversion, no mainnet arm
    address.rs   an address never separated from its network
  tests/
    wiring.rs         what is actually ours: paths, pinned addresses, networks
    mainnet_guard.rs  §2, enforced behaviourally and structurally
```

## This crate implements no BIP

bech32, BIP-32, BIP-39 and base58check come from `rust-bitcoin` and `bip39`.
Elliptic curve arithmetic comes from `libsecp256k1` — the same audited C library
Bitcoin Core uses — via the `secp256k1` crate.

What this crate owns is the **wiring**, and that is the only place its own bugs
can live:

- which network's coin type a derivation uses (`bip84_path`)
- that an address is never separated from the network it belongs to (`Address`)
- that no code path can reach mainnet (`network`)

### Why it stopped hand-rolling

An earlier version wrote bech32, HMAC, PBKDF2, BIP-39 and BIP-32 by hand,
deliberately, because §1 values understanding over adoption. That was defensible
while the only user was the author and §2 made being wrong free.

It shipped a real bug. BIP-39 requires NFKD normalisation of the mnemonic and
passphrase; the hand-written version omitted it, so a passphrase containing a
combining accent silently derived a **different wallet** from every other
implementation — no error, and no way for a user to notice until their coins
were somewhere they could not reach. It was found only by testing against the
crates that now do the work.

Real users make that trade indefensible. These libraries have years of review
and bug reports behind them; the hand-written versions had none. §9 records the
decision.

**The learning was not lost by deleting the code.** It is why `to_seed` is
called rather than `to_seed_normalized`, and why that line carries a comment.

### Swapping in a library did not automatically fix the bug

The first version of the swap called `Mnemonic::to_seed_normalized(passphrase)`.
That function means "the passphrase is *already* NFKD-normalised" and skips the
step — reintroducing precisely the original bug, now wearing a well-reviewed
library's name.

It was caught on the first test run, by the regression test kept from the
hand-rolled era. Libraries remove the burden of implementing a standard
correctly. They do not remove the burden of calling them correctly, and the
tests that survive a rewrite are the ones that know the difference.

## §2 after the swap

Every standard Bitcoin library knows about mainnet. There is no testnet-only
one. So linking `rust-bitcoin` put `Network::Bitcoin`, the `bc` prefix and coin
type 0 into the binary, and §2's original wording — "those values do not exist
in the binary" — stopped being literally true.

That is a real weakening, and it was restated rather than quietly dropped:

> No Thadam type can name mainnet, and exactly one function converts to the
> library's network type.

`src/network.rs` holds that function. It is an exhaustive match over
`BitcoinNetwork`, which has no mainnet variant, so no input produces
`Network::Bitcoin` — and adding a mainnet variant upstream would stop it
compiling.

A narrower guarantee needs *more* enforcement, not less, so
`tests/mainnet_guard.rs` checks it two ways:

- **Behaviourally** — exhausts the public API across every network, phrase and
  index, asserting nothing `bc1`-shaped comes out, no mainnet address parses,
  and no path derives at coin type 0.
- **Structurally** — reads `src/` and fails if any file outside the two boundary
  files names the library's network types, or if any file names a mainnet
  constant. A source-reading test is unusual; it is here because the rule is
  architectural rather than functional, and the alternative is a comment nobody
  is obliged to obey.

## How it is verified

The old differential suite was **deleted, not ported**. It compared this crate's
output against `rust-bitcoin`; now that `rust-bitcoin` is the implementation,
those tests would compare a library to itself — green forever, proving nothing.
A passing suite that cannot fail is a false statement about how well the code is
tested.

What replaced it tests the wiring:

| test | catches |
|---|---|
| pinned addresses for the standard phrase | any change to purpose, coin type, path shape or address type |
| `bip84_paths_use_the_registrys_coin_type` | a path that disagrees with the network it claims |
| `every_path_component_changes_the_address` | an argument silently dropped |
| `passphrases_…_normalise_consistently` | the NFKD trap, in both its forms |
| `testnet4_and_signet_…_indistinguishable` | §8's hazard, stated as a passing test |

The pinned addresses are meaningful for the first time now. While this crate
computed its own, pinning one only asserted the code still did what it did
yesterday. With the cryptography in a library, a pinned address tests *path
assembly*.

### The guards were mutation-tested

| mutation | caught by |
|---|---|
| `BIP84_PURPOSE` 84 → 44 | derivation path guard |
| chokepoint maps testnet4 → mainnet | address parsing + structural guard |
| `to_seed` → `to_seed_normalized` | NFKD regression test |
| `bitcoin::Network` used outside the boundary | structural guard |

The second is worth noting: it slipped past the `bc1` check, because the HRP is
selected separately from the network. Two other guards caught it. That is
layered defence working rather than luck, and it is the reason the guard file
checks the same rule more than one way.

## Running it

```sh
cargo test --release
cargo clippy --all-targets -- -D warnings
```

## Known gaps

- **No signing.** Nothing to sign until a transaction builder exists, so the
  signer *port* (§3) is not defined either — §10 says abstractions are
  extracted, not anticipated.
- **No storage.** §5's encrypted-at-rest phrase, password KDF and lock/wipe
  semantics are absent. This crate derives; it does not persist.
- **No entropy source.** `mnemonic_from_entropy` takes bytes from the caller.
  Where randomness comes from belongs at the edge of the system, and getting it
  from the wrong place is how real wallets have failed.
- **Zeroization is absent.** §8 already states that best-effort zeroization in
  WASM linear memory is unreliable. Nothing here pretends otherwise.
- **Three registry constants remain unverified.** `base58_p2pkh_version`,
  `base58_p2sh_version` and `base58_wif_version` are transcribed but exercised
  by nothing, because Thadam's addresses are bech32 and nothing emits a WIF key.
