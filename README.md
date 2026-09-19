# Thadam — Architecture

**Status:** chain registry and signer built — Bitcoin testnet4, recovery phrase to receive address, on standard libraries. Transaction builder, wallet session, UI and chain service not started.

**Scope of this document:** the decisions that constrain how code is written. It is not a roadmap and not a feature list. Decisions already taken and their reasoning are in Appendix A; what remains open is §9.

Read this before writing any code in this repository.

---

## 1. What Thadam is

A workspace for interacting with blockchain **test networks**: create a wallet, fund it from a faucet, send transactions, and inspect what actually happened.

It is built **for people other than its author**. That is a change from how this document started, and it decides arguments rather than describing a mood: when clarity for the author and safety for a user conflict, the user wins. The first time that came up — hand-written BIP implementations against reviewed libraries — the libraries won, and a real bug went with them (Appendix A.4).

Understanding is still why the code is written the way it is: long comments, explicit types, and modules named for the concept rather than the layer. That is a motivation, not a criterion. A thing that teaches well and loses someone's testnet coins has failed.

It is **non-custodial**. No server ever holds key material. This is not a preference — it is the definition of the product, and no feature is worth breaking it for.

---

## 2. The rule that overrides everything

**No Thadam type can name mainnet, and exactly one function converts to a library's network type.**

There is no mainnet variant in any Thadam enum. No config file, environment variable, query parameter or UI toggle can select mainnet, because no value of `BitcoinNetwork` or `EnvironmentId` denotes it, and none can be parsed into existence.

Where the guarantee lives:

| | property | enforced by |
|---|---|---|
| `registry/core` | mainnet constants are not in the artifact at all | zero dependencies; `tests/no_mainnet.rs` |
| `signer` | mainnet exists in a dependency but is unreachable | `src/network.rs` is the only conversion; `tests/mainnet_guard.rs` |

The signer's guarantee is the weaker of the two, deliberately. Every standard Bitcoin library knows about mainnet and no testnet-only one exists, so using reviewed cryptography means `Network::Bitcoin` and the `bc` prefix sit in the binary unused. That trade is correct — unreviewed cryptography is a worse risk than an unused constant — but it is a trade, and it is why the guard file checks the rule behaviourally *and* by reading the source for bypasses.

A build that cannot touch real money can be wrong without being expensive. Every safety allowance in §8 depends on that.

Adding a mainnet variant is not a feature request. It invalidates this document and the threat model behind it. See §9 for the gate that decision has to pass.

---

## 3. Layers

```
┌─ Client trust boundary ────────────────────┐
│                                            │
│  React UI          renders state,          │
│                    captures intent         │
│        │                                   │
│  Wallet session    owns the flow           │
│                    (state machine)         │
│        │                                   │
│  Transaction       pure logic,             │
│  builder           holds no keys           │
│        │                                   │
│  Signer            the only layer          │
│                    with keys               │
│                                            │
└────────────────────────────────────────────┘
                    │
            Go chain service        data + broadcast
                    │
              Networks              testnet4, sepolia, ...
```

Dependencies point one way. Nothing below imports something above it.

### React UI

Renders whatever state the session reports. Sends user intent back.

**May not:** compute balances, select coins, calculate fees, construct transactions, decide when to sign, call a chain RPC directly, or hold key material.

**Test for this layer:** the entire send flow must be drivable from a script with no React present. If it is not, logic has leaked upward.

### Wallet session

Owns the sequence. A state machine:

```
Empty → Draft → Reviewed → Signed → Broadcast
```

Illegal transitions do not exist. Signing an unreviewed draft is not a guarded branch — it is an absent transition, so it cannot be reached by a UI bug or a refactor.

### Transaction builder

Takes coins and intent, returns an unsigned payload plus a review summary.

Deterministic. No network access. No key access. Chain-specific: Bitcoin's builder and an EVM builder share no code and should not be forced to.

### Signer

The only layer with keys. Owns the recovery phrase, derivation, and address encoding.

**It implements no BIP.** bech32, BIP-32, BIP-39 and base58check come from `rust-bitcoin` and `bip39`; curve arithmetic comes from `libsecp256k1`. What it owns is the wiring — which network's constants a derivation uses, that an address is never separated from its network, and that no path reaches mainnet — and that is where its own bugs can live.

**The signer port is not defined yet.** It becomes an interface when there is a payload to hand it, which requires the transaction builder. §10: abstractions are extracted, not anticipated. When it does exist:

| Implementation | Status | Holds keys |
|---|---|---|
| WASM in browser | V1 | in browser memory, encrypted at rest |
| Hardware wallet | future | on device |
| Remote HSM / MPC | future | in the service |

Nothing above the port changes when the implementation is swapped. That is the port's entire purpose, and it is why the browser's weak security model does not become permanent.

**The port will take a structured payload, never a bare hash.** A signer that sees only a hash cannot show the user what they are approving. Hardware wallets exist to display it.

### Go chain service

Answers questions about chains. Relays finished transactions.

**May not:** hold keys, sign, decide when the client signs, or receive intent, seed material, or passwords.

---

## 4. Verify before sign

Whatever is about to be signed must be checked locally against what the user approved.

The review summary is derived from **the actual payload bytes**, never from the intent that produced them. If a builder outside the trust boundary ever supplies a payload, the client rebuilds and verifies it before the signer is called.

Signing something you did not verify is blind signing. It is how real custody systems lose funds.

---

## 5. Keys, storage, recovery

**The recovery phrase is the wallet.** Twelve words, generated client-side. Every key on every chain derives from them by public standard. Same words, same keys, in Thadam or in any other standard wallet.

That last sentence is a compatibility obligation, not a description. It is what the signer's pinned-address tests protect: a change that alters derivation strands a user's coins in a wallet they can no longer open anywhere else.

**On disk:** only the phrase encrypted with the user's password, via a deliberately slow key-derivation function. The password itself is stored nowhere.

**Unlocked:** phrase and derived keys exist in memory only. Lock, tab close, or timeout wipes them.

**Recovery:** re-entering the phrase. There is no other path. No account, no login, no server-side restore — a server able to restore a wallet is a custodial server.

**Multi-chain from one phrase:** different derivation paths, one backup. Bitcoin and EVM keys differ; the phrase behind them does not.

**Entropy comes from the edge.** The signer takes entropy as an argument rather than sourcing it. Where randomness comes from is a decision to make deliberately, in one visible place — getting it from the wrong place is how real wallets have failed. That place does not exist yet.

### Phrase handling rules

- Never transmitted: not to the API, analytics, error reporting, or logs.
- Never in a URL or query parameter.
- Shown at creation, before the wallet is usable, with a verification step (type back N words at random positions).
- Shown afterwards only from settings, behind password re-entry.
- The creation screen states plainly that Thadam cannot recover it.
- The creation screen teaches the phishing rule: Thadam never asks for these words again except when importing a wallet.

---

## 6. Chain registry

The catalogue of chains and their environments. **Built** — `registry/`, Bitcoin only.

Two levels — a chain, and the environments within it:

- Bitcoin → testnet4, signet, regtest *(built)*
- Ethereum → sepolia, holesky, local devnet *(not built)*
- Solana → devnet, testnet, local validator *(not built)*

Ethereum and Solana are absent from the code, not merely unfinished. Entries nothing consumes are the structure-before-content §10 rejects, and their constants would be unverified.

**The registry is lookup only.** It finds an environment and lists what exists. It does not know how to build a transaction for any chain. The moment chain logic enters the registry, it has become the premature abstraction this design avoids.

**The registry holds constants, not configuration.** Endpoints — RPC, explorer, faucet — are runtime config validated against the compiled catalogue, and are not in it. Consensus constants change never and fail silently; endpoints change per deploy and fail loudly. Appendix A.1.

### No cross-chain abstraction yet

Chains differ in kind, not degree:

| | Bitcoin | EVM | Solana |
|---|---|---|---|
| State model | UTXO set | account balance | accounts + rent |
| Tx input | selected UTXOs | nonce | recent blockhash |
| Fee | sat/vB × vsize | gas × price | lamports + compute units |
| Address | bech32 / bech32m | keccak checksum hex | base58 pubkey |
| Signature | ECDSA / Schnorr | ECDSA over RLP | ed25519 |
| Change | explicit output | none | none |

Any interface wide enough to cover all three collapses to "make bytes, sign bytes, send bytes" and teaches nothing.

**Sequence:** one chain end to end with zero abstraction → a second chain written independently and deliberately duplicated → only then extract what is genuinely shared. Abstractions are extracted after two implementations, never designed before the first.

Ethereum is where this rule gets uncomfortable, and the discomfort is the point. Expect a second derivation path, a second address encoder and a second builder that look like they want to share code. Do not let them until both work.

What *is* legitimately shared: chain/environment identity and parameters, the phrase-to-key derivation chain, the build → review → sign → broadcast lifecycle, network carried as explicit state, and a data provider per environment.

---

## 7. Deployment

Two hosted things:

1. **Static frontend** — React build plus the `.wasm` asset. Files on a CDN.
2. **Go chain service** — one process. Chain data, broadcast relay, health checks as goroutines, not separate deployments.

No database until something needs to persist. Health-check history in memory to start. No Bitcoin node of our own — point at a public provider.

---

## 8. Known limitations, stated as facts

These are not mitigated risks. They are accepted because **the assets are worthless**, and that is the only thing making them acceptable.

- A browser wallet cannot protect keys from XSS on its own origin. WASM does not change this; the language is not the boundary, the browser is.
- Zeroization of key material in WASM linear memory is best-effort and unreliable.
- A malicious browser extension can read the screen, including the recovery phrase.
- Screenshots of the phrase cannot be prevented.

Do not describe this wallet as secure because tests pass. Document what remains broken.

**What worthless assets do not excuse.** §1 changed: there are users who are not the author. Lost testnet coins remain a non-event. A recovery phrase leaked into a log, an analytics payload or a URL is not, because people reuse phrases across wallets no matter how often they are told not to. §5's phrase handling rules are not in the same category as the list above and are not covered by this section's excuse.

**Testnet address collision:** testnet3, testnet4 and signet share identical address encoding (`tb1`, same base58 version bytes). An address does not identify its chain. Sending to the wrong one succeeds silently and the coins are gone. Network is explicit state, never inferred from an address, and is displayed next to every address in the UI. Regtest is the exception — `bcrt1` is distinguishable. The signer has a test asserting this collision still holds, so it cannot be quietly forgotten.

**Cryptographic review:** the BIP implementations are `rust-bitcoin`, `bip39` and `libsecp256k1` — widely used, with years of bug reports behind them. Thadam's own code is not audited, and the wiring around a reviewed library is still somewhere a bug can live (Appendix A.4).

---

## 9. Open decisions

- **Endpoint values.** The shape is settled — runtime config keyed by environment id, rejected at load if the key is not in the compiled catalogue. No provider URL has been written down, because none has been verified against a live service and §10 rejects invented APIs. Blocks the Go chain service.

- **Faucet integration.** §1 names funding from a faucet as part of the product. Whether Thadam links out, proxies a request through the chain service, or does nothing is undecided. A proxy would make the service hold a rate-limit credential — the first thing it would hold that is not public data.

- **When Ethereum starts.** After Bitcoin is end-to-end, per §6. Whether "end to end" means broadcast working or the UI finished is not settled.

- **Whether mainnet is ever added.** Deliberately last, and deliberately hard. This is a gate, not a pull request. Before it can be considered: an external security review, a trusted entropy source built and reviewed, §5's encrypted storage actually implemented, and every item in §8 re-read as though the assets had value — because most of them would no longer be acceptable. Written down now, while it is cheap and hypothetical, because at the decision point there will be users and momentum arguing against it.

---

## 10. Working agreement

- Discussion and explicit permission precede implementation. No code is written ahead of an agreed step.
- Abstractions are extracted, not anticipated. No structure exists before something goes in it.
- A correctness or security problem stops forward progress.
- Placeholder security, fake chain responses in production paths, and invented APIs are rejected outright.
- Prefer a standard, well-reviewed library. Hand-written cryptography or encoding requires a recorded decision in Appendix A, not a preference.
- A test that cannot fail is deleted, not kept. A green suite that proves nothing is a false statement about coverage.

---

## Appendix A. Decisions taken

Recorded because the reasoning outlasts the outcome, and because two of these reverse an earlier decision — those are the entries worth reading.

### A.1 The registry holds constants; endpoints are runtime config

The original plan was TypeScript-canonical, with Rust holding a verified copy of the constants it needs and an agreement test between them.

The premise was wrong. Counting consumers showed the consensus constants have exactly one — the signer. The UI renders names and derives nothing; the chain service relays bytes to a URL. A service that validates an address is a service you are now trusting to validate it correctly (§4), so it does not get them either. With one consumer there is no copy to keep in sync: `registry/core` holds the original and nobody else needs one.

What *is* duplicated is environment identity — three strings and their display names — hand-maintained per language and guarded by `registry/conformance/`. Codegen would buy nothing at that size; revisit past roughly ten environments.

The constants/endpoints split came from the same exercise. Consensus constants never change and fail silently; endpoints change per deploy and fail loudly. Only the first kind is compiled in, which is what §2 requires.

### A.2 Environment identifier is `chain:environment`

`bitcoin:testnet4` as the wire and persistence form, parsed into a typed value at every boundary so no code does string surgery on one.

The worry that this becomes a breaking change once persisted was overweighted: the only things that will persist an identifier are the encrypted phrase blob and a last-selected-network value, both trivially migratable.

### A.3 Bitcoin testnet4 first

Hardest of the three — UTXO selection, change outputs, vsize fees, and the address collision in §8 — and therefore the one that exercises the most of this architecture.

### A.4 Standard libraries, not hand-written BIP mechanics

**This reverses an earlier decision.** The first signer hand-rolled bech32, HMAC, PBKDF2, BIP-39 and BIP-32, on the reasoning that understanding was the success criterion. That was defensible while the only user was the author.

It shipped a real bug. BIP-39 requires NFKD normalisation of the mnemonic and passphrase. The hand-written version omitted it, so a passphrase containing a combining accent silently derived a **different wallet** from every other implementation — no error, and no way for a user to notice until their coins were somewhere they could not reach. It was found only by differential testing against the crates that now do the work.

Two things are worth keeping from that episode.

**Swapping to the library did not fix the bug.** The first version of the swap called `Mnemonic::to_seed_normalized`, which means "the passphrase is *already* normalised" and skips the step — reintroducing the identical bug, wearing a reviewed library's name. It was caught by the regression test kept from the hand-rolled era. Libraries remove the burden of implementing a standard correctly, not of calling it correctly.

**§2 had to be weakened, and not quietly.** Linking `rust-bitcoin` puts mainnet constants in the binary, so "those values do not exist in the binary" stopped being true. The rule was restated and given more enforcement rather than less. See §2.

### A.5 The differential suite was deleted rather than ported

It compared the hand-written implementation against `rust-bitcoin`. Once `rust-bitcoin` became the implementation, those tests compared a library to itself: green forever, proving nothing.

They were replaced by tests of the wiring — pinned addresses, path assembly from registry constants, network separation — which is the only part that is Thadam's to get wrong. §10's rule about tests that cannot fail comes from here.
