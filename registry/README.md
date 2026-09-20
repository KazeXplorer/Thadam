# Registry

The catalogue of Bitcoin networks and their parameters. README §7.

```
registry/
  src/
    bitcoin.rs      the networks and their constants
    id.rs           chain:network identifiers
    environment.rs  one catalogue entry
    lib.rs          the catalogue
  tests/
    network_safety.rs  §2 — a network is never chosen for the user
    catalogue.rs       internal consistency
```

**Zero dependencies, and that must stay true.** This crate holds the constants
that decide which chain an address belongs to. A deserialization dependency
would be the first step toward loading them from data at runtime, which is how
a user's network gets decided by whatever wrote a file last. `network_safety.rs`
fails if the dependency list is non-empty.

## What this crate is

Lookup only. It answers which networks exist and what their parameters are. It
does not derive keys, encode addresses, build transactions or reach a network.
The moment chain logic enters here it has become a premature abstraction.

## The networks

| network | prefix | coin type | role |
|---|---|---|---|
| `mainnet` | `bc` | 0 | real money |
| `testnet4` | `tb` | 1 | the test chain users are given |
| `signet` | `tb` | 1 | staging, before mainnet |
| `regtest` | `bcrt` | 1 | integration tests, no shared history |

testnet3 is absent: its difficulty reset rule makes it unusable for
timing-sensitive work, and its addresses are indistinguishable from testnet4's.

**The coin type is what separates real money from test money.** One recovery
phrase derives two entirely unrelated wallets, because BIP-84 puts mainnet at
coin type 0 and every test network at 1. A test asserts this, because if the
two ever matched, testnet experimentation would expose real keys.

## §2: a network is never chosen for the user

This crate carries three of the four parts:

**No `Default`.** A default is a network chosen by the code. `BitcoinNetwork`
has none and must not grow one — `network_safety.rs` reads the source and fails
if `Default` is derived or implemented.

**Exact parsing, no aliases.** `"MAINNET"`, `"main"`, `"bitcoin"`, `"prod"` and
`"mainnet "` all fail to parse. A generous parser is how a malformed deep link
or a stray config value picks a chain.

**No fallback arms.** Every match over a network is exhaustive, so adding a
network cannot silently inherit another's constants. Adding a variant breaks
the build in every place that has to decide something about it — which is the
intended behaviour, not an inconvenience.

The fourth part — that nothing external can change the network — lives above
this crate, in the signer's chokepoint and the app.

## Constants, not configuration

Endpoints — RPC, explorer, faucet — are runtime config validated against this
catalogue, and are **not** in it.

| | changes | a wrong value causes |
|---|---|---|
| consensus constants (here) | never | a well-formed address on the wrong chain, silently |
| endpoints (elsewhere) | per deploy | a request to the wrong server, loudly |

## What these tests replaced

`tests/no_mainnet.rs` used to assert that no mainnet constant existed anywhere.
That was the old §2, from when Thadam was testnet-only.

It was deleted rather than adjusted. The distinction matters: those tests would
have kept **passing** after mainnet was added, because nothing about them was
wrong — they had simply stopped describing a rule anyone still held. A green
suite that guards nothing is a false statement about coverage (§11).

`network_safety.rs` guards the rule that replaced it, and was mutation-tested
against it: setting mainnet's coin type to 1, or making the parser
case-insensitive, both fail.

## Also removed

`registry/ui-catalog` (TypeScript) and `registry/conformance` are gone.
Decision A.6 moved Thadam to native mobile, so the catalogue reaches Swift and
Kotlin through UniFFI from Rust rather than through a duplicated TypeScript
list — which leaves one implementation and nothing for a conformance check to
compare. `src/bin/dump_environments.rs` went with it; it existed only to feed
that check.

## Running it

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```
