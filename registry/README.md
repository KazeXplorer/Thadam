# Chain registry

The catalogue of chains and the environments within them. ARCHITECTURE.md §6.

```
registry/
  core/         every constant, the full catalogue, the §2 guard   (Rust)
  ui-catalog/   environment identity for display                   (TypeScript)
  conformance/  the agreed fixture + the drift check
```

## What the registry is

Lookup only. It answers two questions:

- which environments exist, and what do we call them
- what are this environment's address and key-encoding parameters

It does not derive keys, encode addresses, build transactions, or reach a
network. The moment chain logic enters here it has become the premature
abstraction §6 warns against.

## The two kinds of data, and why they are split

The registry looked like one thing and is two:

| | changes | a wrong value causes |
|---|---|---|
| **Consensus constants** — HRP, version bytes, coin type | never | a well-formed address on the wrong chain, silently |
| **Operational config** — RPC URLs, explorers, faucets | per deploy | a request to the wrong server, loudly |

Only the first kind is in this directory, and it is **compiled in**.

That is a direct consequence of §2. If the signer read its HRP from a JSON
file at runtime, then `"hrp": "bc"`, coin type `0` and version byte `0x00`
would be one file edit away from a mainnet wallet. §2 says no config file can
reach mainnet parameters "because those values do not exist in the binary" — a
runtime-loaded registry *is* that config file. So the constants live in a
closed Rust enum with no mainnet variant, and `tests/no_mainnet.rs` fails the
build if a mainnet value ever appears.

Endpoints are not here. They are supplied at runtime and validated against
this catalogue. They are not built yet — the Go service that consumes them
does not exist.

The names say the hierarchy: `core/` is the real registry, `ui-catalog/` is a
narrow slice cut for one consumer. They are not two versions of one thing, and
naming them `rust/` and `ts/` implied they were.

## Why the constants are in `core/` and nowhere else

ARCHITECTURE.md §9 framed this as a cross-language synchronisation problem —
TypeScript canonical, Rust holding a verified copy, a test asserting the two
agree. Counting the actual consumers dissolves it:

| consumer | needs consensus constants? |
|---|---|
| WASM signer (Rust) | **yes** — it derives keys and encodes addresses |
| React UI | no — it renders names and derives nothing |
| Go chain service | no — it relays bytes to an RPC URL |

The Go service would need them only to validate an address before broadcast,
and it should not: §4 puts verification inside the trust boundary, and a
server that checks your address is a server you are now trusting to check it
correctly.

So there is no copy to keep in sync. `core/` holds the original; nobody else
needs one.

## What *is* duplicated, and why that is fine

The identity slice — the three `chain:environment` strings, their display
names, and whether each is live or local. Three entries, nothing
cryptographic, maintained by hand in each language.

`conformance/check.sh` has each implementation dump its catalogue as canonical
JSON and diffs both against `conformance/environments.json`. Drift fails.

Codegen would buy nothing at this size and would add a build step to a
directory whose whole point is that its contents are auditable by reading
them. Revisit past roughly ten environments — and note that the escape hatch
is codegen that *emits compiled constants at build time*, so §2 survives.

## Identifiers

`chain:environment`, e.g. `bitcoin:testnet4`. That string is the wire and
persistence form. Both implementations parse it into a typed value at the
boundary so no code does string surgery on an identifier.

`bitcoin:mainnet` is not rejected by a validation rule. It is unconstructable:

- **Rust** — `EnvironmentId::Bitcoin(BitcoinNetwork)`, and `BitcoinNetwork`
  has no `Mainnet` variant. Adding one is a compile error in every exhaustive
  match before any test runs.
- **TypeScript** — `EnvironmentId` is the template literal type
  `` `bitcoin:${BitcoinNetwork}` `` derived from a `const` array, so
  `"bitcoin:mainnet"` is a type error. The runtime parser is tested too,
  because identifiers arrive from persisted state and URLs where the compiler
  cannot see them.

## What is in the catalogue

| id | kind | HRP | note |
|---|---|---|---|
| `bitcoin:testnet4` | live | `tb` | |
| `bitcoin:signet` | live | `tb` | parameters identical to testnet4 |
| `bitcoin:regtest` | local | `bcrt` | the only one its address identifies |

testnet3 is absent: its difficulty reset rule makes it unusable for
timing-sensitive work, and its addresses are indistinguishable from
testnet4's. Ethereum and Solana are absent: §6 mandates one chain end to end
with zero abstraction first, and entries nothing consumes are the
structure-before-content §10 rejects.

`tests/no_mainnet.rs` asserts that testnet4 and signet stay identical. That
test exists so the §8 collision cannot be quietly forgotten — if it ever
fails, the warnings written around that hazard need revisiting.

## Running it

```sh
# core — constants, identity, and the §2 guard
cd core && cargo test && cargo clippy --all-targets -- -D warnings

# ui-catalog — identity (needs Node >= 22.6, no dependencies)
cd ui-catalog && npm test

# Cross-language drift check
./conformance/check.sh                 # strict; a missing toolchain fails
./conformance/check.sh --allow-missing # local dev
```

Strict is the default because a conformance check that quietly skips the
implementation that drifted is worse than no check.

## Verified, not assumed

The guards were mutation-tested rather than trusted:

| mutation | caught by |
|---|---|
| testnet4 HRP → `"bc"` | `no_bitcoin_network_carries_a_mainnet_constant` |
| coin type → `0` | same |
| xpub version → `0x0488B21E` | same |
| a `Mainnet` enum variant added | compile error, `tests/catalogue.rs` |
| a `ui-catalog/` display name changed | `conformance/check.sh`, exit 1 |

## Still open

- **Go.** Not written. It would join as a third sibling here. The service does not exist; the fixture is already the
  thing its dump will have to agree with.
- **Endpoints.** Shape agreed (runtime config, keyed by environment id,
  rejected at load if the key is not in the compiled catalogue), values not
  chosen. No provider URL has been written down, because §10 rejects invented
  APIs and none has been verified against a live service yet.
- **Constant provenance.** The values are transcribed from Bitcoin Core
  `chainparams.cpp`, BIP-32 and SLIP-0044, and cited at each field. Nothing
  here has been differentially tested against a known-good library, because
  nothing here encodes anything yet. That test belongs with the signer.
