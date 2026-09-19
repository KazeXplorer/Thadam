# Language Workflow — Why the Registry Code Is Shaped This Way

This is not a Rust or TypeScript tutorial. It explains the design decisions
behind Thadam's registry — why certain language features were chosen, what
they buy you, and what goes wrong if you pick the alternative.

---

## 1. Why Two Languages for the Same Data?

The registry exists in two copies: `core/` (Rust) and `ui-catalog/` (TypeScript).

**Rust side** — carries the crypto constants (bech32 HRP, version bytes, BIP-44
coin type). This compiles into WASM and runs inside the signer. The constants
are used during address derivation and transaction signing — getting one wrong
means money goes to a wrong address or a signature fails to verify. So the
language holding them must guarantee they're checked at compile time, not at
runtime when a user is staring at a broken wallet.

**TypeScript side** — carries only identity data (names, display strings, which
environments exist). The UI needs to know "what can I show in a dropdown" and
"what do I call this network." It never touches a private key or encodes an
address. So it deliberately excludes the crypto constants — not because TS
can't hold them, but because duplicating them creates a second copy that can
drift and no consumer on the UI side needs them.

**The tradeoff being made:** duplication of the environment *list* (3 entries)
is cheap and verified by `conformance/check.sh`. Duplication of the crypto
*constants* would be expensive — two copies of bech32 HRPs and version bytes
that must stay identical, with no way to enforce that across language boundaries
at compile time. So: duplicate the cheap part, single-source the dangerous part.

---

## 2. Closed Enum vs Config File

The central design choice: `BitcoinNetwork` is an enum with exactly three
variants (Testnet4, Signet, Regtest), not a JSON/TOML config file loaded at
startup.

### What a config file would look like

```
# networks.toml
[bitcoin.mainnet]
bech32_hrp = "bc"
coin_type = 0

[bitcoin.testnet4]
bech32_hrp = "tb"
coin_type = 1
```

### Why this was rejected

A config file means the list of networks is a **runtime** decision. Any
deployed binary can touch mainnet if someone edits a file. The guarantee
"this wallet cannot touch real money" becomes a promise about deployment
discipline, not a property of the software.

With a compiled enum:
- **Mainnet is unconstructable**, not just unconfigured. There is no value of
  `BitcoinNetwork` that represents mainnet. It's not filtered out — it doesn't
  exist in the type system.
- **Exhaustive matching** — every `match` on the enum must handle all variants.
  Add a new network and the compiler tells you every place that needs updating.
  With a config file, you'd need to grep for string comparisons and hope you
  found them all.
- **No parsing risk** — a typo in a config file (`bech32_hrp = "bc"` instead of
  `"tb"`) silently creates a mainnet wallet. A typo in an enum variant doesn't
  compile.

### When you'd pick a config file instead

If Thadam supported 50 chains and users could add custom RPCs, a compiled enum
would mean recompiling for every new chain. Config files win when the list is
large, user-controlled, or changes faster than your release cycle. Thadam's list
is small, developer-controlled, and safety-critical — so compiled wins.

---

## 3. `const fn` — Compute at Compile Time

```rust
pub const fn params(self) -> BitcoinParams {
    match self {
        BitcoinNetwork::Testnet4 | BitcoinNetwork::Signet => BitcoinParams {
            bech32_hrp: "tb",
            ...
        },
        BitcoinNetwork::Regtest => BitcoinParams {
            bech32_hrp: "bcrt",
            ...
        },
    }
}
```

`const fn` means this function *can* run at compile time. The compiler
evaluates it and bakes the result into the binary.

**What this buys you:** The params aren't computed when a user opens the wallet.
They're already in the binary as fixed data. No allocation, no parsing, no
failure path. In WASM (where Thadam's signer runs), this matters — you want
the smallest, fastest, most predictable code path between "user clicks sign"
and "signature is produced."

**What the alternative looks like:** A regular function that reads from a
HashMap or matches strings at runtime. Works fine, but adds a failure path
(what if the key isn't in the map?) and a startup cost (building the map).
For three networks, the cost is trivial — but the *failure path* is the real
problem. `const fn` means there's no path where params aren't available.

---

## 4. `FromStr` — Rust's Convention for Parsing

```rust
impl FromStr for BitcoinNetwork {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "testnet4" => Ok(BitcoinNetwork::Testnet4),
            "signet" => Ok(BitcoinNetwork::Signet),
            "regtest" => Ok(BitcoinNetwork::Regtest),
            _ => Err(()),
        }
    }
}
```

This isn't special syntax — it's Rust's standard trait (interface) that says
"this type can be created from a string." Any type that implements `FromStr`
gets `.parse()` for free:

```rust
let network: BitcoinNetwork = "testnet4".parse().unwrap();
```

**Why use a trait instead of a standalone function like `parse_network(s)`:**
- Every Rust developer already knows `FromStr`. A standalone function is one
  more name to learn.
- Generic code can work with anything parseable: `fn load<T: FromStr>(input: &str)`.
- The `_ => Err(())` arm is where mainnet dies. `"mainnet".parse::<BitcoinNetwork>()`
  returns `Err(())` — not because of a validation check, but because no arm
  matches it. The same principle as the enum: mainnet isn't rejected, it's absent.

**The TypeScript equivalent** is a plain function (`parseEnvironmentId`) because
TS doesn't have traits. Same logic, different mechanism.

---

## 5. `as const` — TypeScript's Way to Lock Down Values

```typescript
export const BITCOIN_NETWORKS = ["testnet4", "signet", "regtest"] as const;
export type BitcoinNetwork = (typeof BITCOIN_NETWORKS)[number];
```

Without `as const`, TypeScript infers the array type as `string[]` — it knows
there are strings in there but doesn't know which ones. With `as const`, it
infers the type as the literal tuple `readonly ["testnet4", "signet", "regtest"]`.

Then `(typeof BITCOIN_NETWORKS)[number]` extracts a union type:
`"testnet4" | "signet" | "regtest"`.

**What this buys you:** `"bitcoin:mainnet"` becomes a **type error**, not a
runtime check. The type `EnvironmentId` is defined as `` `bitcoin:${BitcoinNetwork}` ``,
which expands to `"bitcoin:testnet4" | "bitcoin:signet" | "bitcoin:regtest"`.
If you write `const id: EnvironmentId = "bitcoin:mainnet"`, TypeScript refuses
to compile. Same principle as Rust's closed enum, adapted to TS's type system.

**Without `as const`:** You'd need runtime validation everywhere — `if (id === "mainnet") throw ...`.
That works, but it's a check that can be forgotten. The type system doesn't
help you, so a typo in a string somewhere silently lets mainnet through.

---

## 6. The Conformance Script — Gluing Two Languages Together

`conformance/check.sh` runs both the Rust binary and the TS script, collects
their JSON output, and diffs it. If the two copies of the environment list
disagree, CI fails.

**Why this exists:** Rust and TypeScript have no shared compile step. You can't
import a Rust enum into TypeScript or vice versa. So the two copies are
verified by a third thing (a shell script) that doesn't trust either one.

**The alternatives and why they were rejected:**
- **Codegen** (generate TS from Rust or vice versa): Adds a build step, a
  template language, and a "which is the source of truth?" question. For 3
  entries of identity data, it's machinery that costs more to maintain than
  the duplication it eliminates. The README says to revisit past ~10 environments.
- **Single-source JSON** (both languages read one file): The Rust side would
  need to parse JSON at compile time or embed it at build time. Possible,
  but the whole point of the compiled enum is that the data is in the type
  system, not in a file. A shared JSON would undo that.

---

## 7. The Test-as-Documentation Pattern

`tests/no_mainnet.rs` isn't just checking for bugs. It's a machine-readable
statement of a security property:

```rust
#[test]
fn testnet4_and_signet_are_indistinguishable_by_address() {
    let testnet4 = BitcoinNetwork::Testnet4.params();
    let signet = BitcoinNetwork::Signet.params();
    assert_eq!(testnet4, signet,
        "testnet4 and signet no longer share an encoding; revisit §8");
}
```

This test doesn't check correctness — it checks a *known hazard*. Testnet4 and
signet addresses look identical (same `tb` prefix). The test documents that
fact and ensures that if it ever stops being true (Bitcoin Core changes one of
them), the build forces someone to revisit the UI decision of always showing
the network name next to addresses.

**The pattern:** tests that assert known risks rather than just correct
behavior. They're cheaper than an audit finding ("did you know these two
networks share an address format?") because they re-check on every build.

---

## Summary — Patterns to Recognize Elsewhere

| Pattern | What it does | When to use it |
|---|---|---|
| Closed enum over config | Makes invalid states unconstructable | Small, safety-critical, developer-controlled lists |
| `const fn` / compile-time eval | Eliminates runtime failure paths | Fixed data that never changes per-deployment |
| Trait impl (`FromStr`) | Plugs your type into the language's conventions | When the language has a standard interface for what you're doing |
| `as const` + template literals | Moves validation from runtime to the type checker | When the set of valid values is small and known at build time |
| Cross-language conformance test | Catches drift between copies | When the same data lives in two languages with no shared compile step |
| Tests-as-risk-documentation | Asserts known hazards, not just correctness | When a property of the code is load-bearing for security |

These aren't Rust patterns or TypeScript patterns — they're design patterns
that happen to use language features to enforce them. You'll see the same
ideas in Go (interface satisfaction), in Python (Literal types + mypy), in
Solidity (function visibility + modifiers). The keywords change, the shapes
repeat.
