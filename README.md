# Thadam — Architecture

**Status:** chain registry, signer and transaction builder built — Bitcoin, recovery phrase to a reviewed unsigned transaction, testnet only so far. Mobile apps, wallet session, signing, chain service and mainnet support not started.

**Scope of this document:** the decisions that constrain how code is written. It is not a roadmap and not a feature list. Decisions already taken and their reasoning are in Appendix A; what remains open is §10.

Read this before writing any code in this repository.

---

## 1. What Thadam is

A **Bitcoin wallet for iOS and Android**, supporting testnet and mainnet. The product vision is a power-user, non-custodial Bitcoin wallet with deep ecosystem integration — Taproot-native, Ordinals/Runes-aware, Lightning in a later phase. The nearest reference point is Solflare's position in the Solana ecosystem: not a beginner wallet, but the tool serious holders and developers reach for.

It is built **for people other than its author**. That decides arguments rather than describing a mood: when clarity for the author and safety for a user conflict, the user wins. The first time that came up — hand-written BIP implementations against reviewed libraries — the libraries won, and a real bug went with them (A.4).

It is **non-custodial**. No server ever holds key material. This is not a preference — it is the definition of the product, and no feature is worth breaking it for.

Understanding is still why the code reads the way it does: long comments, explicit types, modules named for the concept rather than the layer. That is a motivation, not a criterion. A thing that teaches well and loses someone's Bitcoin has failed.

The phased feature plan lives in `ROADMAP.md`. This document constrains how code is written; the roadmap records what gets built and when.

### What changed, and what it cost

Thadam began as a testnet-only developer workspace for Bitcoin, Ethereum and Solana, running in a browser. It is now a Bitcoin-only mobile wallet that handles real money. Three consequences follow, and they are recorded here rather than quietly absorbed:

- **§2 was replaced, not amended.** The old rule was "mainnet is unrepresentable." That rule is gone. Its successor guards a different danger.
- **§9's accepted risks lost their justification.** They were accepted because the assets were worthless. They are not any more.
- **Ethereum and Solana are out of scope**, along with the cross-chain abstraction rules written for them.

---

## 2. The rule that overrides everything

**A user can never be on a network they did not choose.**

Thadam supports mainnet and testnet in one app. That makes accidental network confusion the highest-consequence failure in the system: a user who believes they are on testnet and is not will send real money on a chain where mistakes are free to imagine and permanent in fact.

The rule has four parts, all mandatory:

1. **Network is explicit state, carried in types, never inferred.** No function derives a network from an address, a URL, a saved preference, or a default. `BitcoinNetwork` is an argument everywhere it matters, and the transaction builder refuses coins, recipient and change that do not agree.
2. **Nothing external can change the network.** Not a scanned QR, not a `bitcoin:` URI, not a deep link, not restored state, not a server response. A mainnet address arriving while the app is on testnet is **refused**, never silently switched to. Only a deliberate action in settings changes the network.
3. **Every screen showing money names its network in words.** Balance, receive, send, review. Colour is an aid, not the mechanism — it fails for colourblind users and in screenshots.
4. **The signing moment names the network in the OS prompt.** The Face ID / fingerprint dialog reads "Send 0.05 BTC on MAINNET". It is the last thing seen before the key is unwrapped, and it is drawn by the operating system rather than by Thadam, so an app-layer bug cannot forge it.

### What enforces it

| | mechanism |
|---|---|
| `registry/core` | `BitcoinNetwork` has no default; every environment is an explicit variant |
| `signer` | `src/network.rs` is the only conversion to the library's network type |
| `builder` | mixed-network inputs are a build error, tested |
| mobile | per-network storage; no external input path reaches the network setting |

Guard tests live in `signer/tests/mainnet_guard.rs` and `builder/tests/mainnet_guard.rs`. **These files need rewriting**: they currently assert mainnet is absent, which was the old rule. Their successors assert that no code path changes the network without a deliberate user action. Until that is done, mainnet must not be enabled.

### One thing in our favour

Mainnet and testnet addresses are visibly different — `bc1` against `tb1`. The dangerous ambiguity in Bitcoin's address encoding is testnet4 against signet, which share a prefix (§9). The pair that matters most here is distinguishable at a glance.

---

## 3. Layers

```
┌─ iOS / Android app ─────────────────────────┐
│                                             │
│  SwiftUI / Compose   renders state,         │
│                      captures intent,       │
│                      owns biometrics        │
│         │                                   │
│  ═══════╪═══════ UniFFI boundary ═══════    │
│         │      (no key material crosses)    │
│                                             │
│  Wallet session      owns the flow          │
│                      (state machine)        │
│         │                                   │
│  Transaction         pure logic,            │
│  builder             holds no keys          │
│         │                                   │
│  Signer              the only layer         │
│                      with keys              │
│                                             │
└─────────────────────────────────────────────┘
                     │
             Go chain service     data + broadcast
                     │
               Networks           mainnet, testnet4, signet, regtest
```

Dependencies point one way. Nothing below imports something above it.

### The UniFFI boundary

One Rust core, compiled for `aarch64-apple-ios` and `aarch64-linux-android`, with Swift and Kotlin bindings generated by UniFFI. Everything below the line is shared; everything above is written twice.

**What crosses into Rust:** user intent, and — only at the moment of signing — the unwrapped storage key.

**What crosses back:** addresses, balances, review summaries, signed transactions.

**What never crosses:** the recovery phrase, the seed, and any private key. Swift and Kotlin have no type that can hold one.

### iOS / Android UI

Renders whatever state the session reports. Sends user intent back. Owns the biometric prompt, because only platform code can talk to the Secure Enclave or StrongBox.

**May not:** compute balances, select coins, calculate fees, construct transactions, decide when to sign, call a chain RPC directly, or hold key material.

**Test for this layer:** the entire send flow must be drivable from a Rust test with no app present. If it is not, logic has leaked upward.

### Wallet session

Owns the sequence. A state machine:

```
Empty → Draft → Reviewed → Signed → Broadcast
```

Illegal transitions do not exist. Signing an unreviewed draft is not a guarded branch — it is an absent transition, so it cannot be reached by a UI bug or a refactor.

### Transaction builder

Takes coins and intent, returns an unsigned payload plus a review summary. **Built** — `builder/`.

Deterministic. No network access. No key access. Coin selection is branch-and-bound with a largest-first fallback, working in effective values so each coin's own input fee is accounted for before the search begins. Candidates are deliberately not shuffled, unlike Bitcoin Core: determinism is what makes §4's review trustworthy.

**Per-type input weight is mandatory.** Fee estimation uses virtual size (vsize), which differs by script type. A P2TR key-path input is 57.5 vbytes; a P2WPKH input is 68 vbytes. A builder that uses a single hardcoded weight silently underestimates fees when spending P2TR UTXOs, or overestimates when spending P2WPKH. Each UTXO in the coin set carries its script type, and the builder calculates input weight per-coin. Mixed transactions (P2TR and P2WPKH inputs together) are valid and must produce correct fee estimates.

**Change outputs default to P2TR.** A transaction that generates change always places it in a new P2TR address from the wallet's change chain, regardless of what address type the inputs spend from.

### Signer

The only layer with keys. Owns the recovery phrase, derivation, and address encoding. **Built** — `signer/`, except signing itself.

**It implements no BIP.** bech32/bech32m, BIP-32, BIP-39, BIP-84 (P2WPKH), BIP-86 (P2TR) and base58check come from `rust-bitcoin` and `bip39`; curve arithmetic from `libsecp256k1`. What it owns is the wiring — which network's constants a derivation uses, that an address is never separated from its network, that no path reaches a network nobody chose.

**Address types supported:** P2TR (BIP-86, `bc1p`/`tb1p`) is the **default** for all new wallets. P2WPKH (BIP-84, `bc1q`/`tb1q`) is supported for receiving and for compatibility. See A.15 for the decision and rationale. The signer must never generate a P2WPKH address as a default; the only permitted caller of P2WPKH derivation is compatibility code handling legacy incoming UTXOs or explicit user override.

**Bech32m, not Bech32, for P2TR.** P2TR addresses use Bech32m encoding (BIP-350). The two are not interchangeable. `rust-bitcoin` handles this; the wiring must never pass a P2TR address through a Bech32 path.

**The signer port is not defined yet.** It becomes an interface when hardware wallet support is real (Phase 3). §11: abstractions are extracted, not anticipated.

**The port will take a structured payload (PSBT), never a bare hash.** A signer that sees only a hash cannot show the user what they are approving.

### Go chain service

Answers questions about chains. Relays finished transactions. See §6.

**May not:** hold keys, sign, decide when the client signs, or receive intent, seed material, or the recovery phrase.

---

## 4. Verify before sign

Whatever is about to be signed must be checked locally against what the user approved.

The review summary is derived from **the actual payload bytes**, never from the intent that produced them. `ReviewSummary::derive` takes no `Intent` parameter — there is no argument to pass one through, so summarising the request instead of the payload is not expressible rather than merely discouraged.

This matters because the two can differ, and every way they differ is a way a user loses money. A summary generated from intent would render a correct-looking approval screen for a transaction that pays somewhere else.

The tampering tests in `builder/tests/review.rs` are deliberately **value-conserving**: they move satoshis to an attacker without changing totals or fee, so a review that only sanity-checked the fee would see nothing wrong.

Signing something you did not verify is blind signing. It is how real custody systems lose funds.

---

## 5. Keys, storage, recovery

**The recovery phrase is the wallet.** Twelve words, generated on the device. Every key derives from them by public standard. Same words, same keys, in Thadam or any other standard wallet.

That last sentence is a compatibility obligation, not a description. It is what the signer's pinned-address tests protect: a change that alters derivation strands a user's coins in a wallet they can no longer open anywhere else.

**Recovery is re-entering the phrase. There is no other path.** No account, no login, no server-side restore, and no cloud backup. A server able to restore a wallet is a custodial server. Some users will lose funds; that is the honest consequence of a non-custodial wallet, not a problem to engineer around (A.13).

**Both networks from one phrase.** BIP-84 (P2WPKH) uses coin type 0 for mainnet and 1 for testnet. BIP-86 (P2TR) uses the same coin type convention. Keys and addresses differ entirely between mainnet and testnet; one backup covers both, with no overlap between them.

**Derivation paths:**

| Type | Purpose | Path |
|---|---|---|
| P2TR (default) | External receive | `m/86'/coin_type'/account'/0/index` |
| P2TR | Internal change | `m/86'/coin_type'/account'/1/index` |
| P2WPKH | External receive (compat) | `m/84'/coin_type'/account'/0/index` |
| P2WPKH | Internal change (compat) | `m/84'/coin_type'/account'/1/index` |

`coin_type` is 0 for mainnet, 1 for testnet. The signer has pinned-address tests for every path × network combination. A change that alters any derivation output fails those tests — which is the point.

### What the hardware can and cannot do

**The Secure Enclave cannot hold a Bitcoin key.** It implements P-256, the NIST curve. Bitcoin uses secp256k1. Android StrongBox is the same. So "keys never leave secure hardware" is **not achievable** for Bitcoin on a phone, and this document will not imply otherwise.

What is achievable, and what Thadam does:

```
Secure Enclave / StrongBox   holds a wrapping key, never extractable,
                             released only after biometric check
          ↓ unwrapped AES key crosses into Rust
Rust                         decrypts the seed, signs, wipes
          ↓ signed transaction crosses back
Keychain / Keystore          holds only ciphertext, device-only
```

An attacker needs the physical device **and** the user's face or fingerprint. That is meaningfully stronger than a browser. It is not "the key is in hardware."

### Storage

The seed is encrypted **in Rust** with a key unwrapped from the Enclave, and the resulting ciphertext is stored in Keychain or Keystore with a device-only flag.

Two reasons for the double layer, and neither is defence in depth — both layers share one trust root, so an attacker past biometrics is past both:

1. **A platform mistake stops being fatal.** iOS Keychain items sync to iCloud unless flagged `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`. Get that wrong and what leaves the device is an opaque blob rather than a seed. Keychain misconfiguration is among the most common iOS wallet bugs.
2. **One implementation instead of two.** Shipping both platforms, the encrypted format is written once in Rust and each platform layer stores bytes it does not understand. The alternative is the most dangerous code in the app, written twice.

**A test must fail if the device-only flag is wrong**, in the same way the network guards work. This is not yet written.

### Unlock

**Biometric only.** No app passphrase. Nothing to forget, nothing to phish, and the check is enforced by the OS rather than by Thadam.

**Watch-only by default.** Balances and receive addresses need only the account `xpub`, so the app opens, syncs and shows everything without any biometric prompt. The seed is needed for exactly one thing: signing a spend.

**Per-signature unlock.** The seed is decrypted, used, and wiped within a single operation. It does not persist across transactions, and there is no unlock timeout window to reason about.

### Phrase handling rules

- Never transmitted: not to the chain service, analytics, error reporting, or logs.
- Never in a URL, deep link or query parameter.
- **No crash reporter or analytics SDK may be linked into a build that can hold a seed** without an explicit decision recorded in Appendix A. Crash reporters capture memory.
- Shown at creation, before the wallet is usable, with a verification step (type back N words at random positions).
- Shown afterwards only from settings, behind a biometric check.
- The app-switcher preview is blurred on the phrase and send screens.
- The creation screen states plainly that Thadam cannot recover it.
- The creation screen teaches the phishing rule: Thadam never asks for these words again except when importing a wallet.

**Screenshots cannot be prevented on iOS.** Android's `FLAG_SECURE` works; iOS only permits *detecting* a screenshot after the fact. The phrase screen warns rather than promising protection it does not have.

---

## 6. The chain service, and what it is trusted for

The app sends its addresses to the Go chain service, which queries a chain data provider and relays broadcasts.

**This is non-custodial but not private.** The service sees every user's addresses, balances and full transaction history. Those are different properties and the distinction is often blurred; Thadam states it plainly rather than implying privacy it does not provide. Compact block filters (BIP-158) would fix it and are not implemented (§10).

### What a compromised or lying service can do

**It cannot steal coins.** It never sees keys, and §4's review is derived from transaction bytes, so it cannot redirect a payment.

**It can cost money in exactly one way: the fee rate.** Report 2,000 sat/vB instead of 20 and a user who taps through pays a hundred times too much, irreversibly, to a miner.

> **Mitigation, mandatory:** the app rejects fee rates outside a hardcoded plausible range and requires explicit confirmation for anything unusual. This is the only path from a bad server to lost funds and it closes cheaply.

**It can also** withhold UTXOs so a user cannot spend, and claim a payment confirmed when it has not been. Both are real; neither is theft.

**The service is a liveness and accuracy dependency, not a custody one.** That sentence is the whole of its threat model.

### Address entry

Clipboard hijacking — malware swapping a copied Bitcoin address for the attacker's — is the most common real-world Bitcoin theft, and it needs no access to this app at all. §4 prevents redirection between build and sign; it cannot help if the wrong address was correct from the start.

- **QR scan is the default path** and is immune to clipboard swapping.
- **Paste is allowed**, followed by a mandatory confirmation screen showing the address in full.
- **Addresses are displayed in full, in small groups, never truncated in the middle.** `tb1q…8f3t4` is unverifiable and is what most apps show.
- A scanned or pasted address on a different network is refused, never auto-switched (§2).

---

## 7. Chain registry

The catalogue of networks and their parameters. **Built** — `registry/`.

- Bitcoin → mainnet *(not yet added)*, testnet4, signet, regtest

**Lookup only.** It finds a network and lists what exists. It does not know how to build a transaction. The moment chain logic enters the registry, it has become a premature abstraction.

**Constants, not configuration.** Endpoints — RPC, explorer, faucet — are runtime config validated against the compiled catalogue, and are not in it. Consensus constants change never and fail silently; endpoints change per deploy and fail loudly (A.1).

Signet and regtest earn their keep as staging and integration-test chains respectively, not as user-facing options.

**Address prefixes per network (informational; the signer owns encoding):**

| Network | P2TR prefix | P2WPKH prefix | Encoding |
|---|---|---|---|
| mainnet | `bc1p` | `bc1q` | Bech32m / Bech32 |
| testnet4 | `tb1p` | `tb1q` | Bech32m / Bech32 |
| signet | `tb1p` | `tb1q` | Bech32m / Bech32 |
| regtest | `bcrt1p` | `bcrt1q` | Bech32m / Bech32 |

The testnet4/signet prefix collision (`tb1`) is a known limitation (§9). The P2TR/P2WPKH distinction is visible in the character after `tb1` — `p` vs `q` — but is not a reliable network discriminator; network is always carried as explicit state.

**Future ecosystem identifiers** — not in the registry yet; recorded here so they are not invented ad hoc when the time comes:
- `bitcoin:lightning` — Lightning Network wallet (Phase 5)
- Ordinals and Runes are on-chain Bitcoin and use the same network identifiers; they require chain service extensions, not registry changes.

---

## 8. Deployment

Two hosted things and two apps:

1. **iOS and Android apps** — App Store and Play Store. Code signing, review, and no hot-fixes.
2. **Go chain service** — one process. Chain data, broadcast relay, health checks as goroutines, not separate deployments.

No database until something needs to persist. No Bitcoin node of our own — point at a public provider.

---

## 9. Known limitations, stated as facts

These are not mitigated risks. **They no longer have the excuse they were written under** — Thadam used to be testnet-only, where being wrong cost nothing. Each one now needs a real answer before mainnet, and the ones with no answer are listed as such.

| limitation | status |
|---|---|
| The Secure Enclave cannot hold a Bitcoin key (wrong curve) | inherent; mitigated by biometric-gated wrapping and per-signature decryption |
| Zeroization of key material in memory is best-effort | inherent; reduced by the seed existing only within one signing call |
| Screenshots of the recovery phrase cannot be prevented on iOS | inherent; warned about, not prevented |
| A compromised device defeats every control here | inherent; no mobile wallet answers this |
| Thadam's own code is unaudited | **no answer yet — blocks mainnet** |
| The chain service sees every user's full history | accepted and stated; BIP-158 would fix it (§10) |
| No replace-by-fee: a stuck transaction cannot be bumped | **no answer yet — blocks mainnet** |
| No entropy source is implemented | **no answer yet — blocks everything** |
| P2TR key-path spend only in v1; script-path spend (Tapscript) not supported | accepted for Phase 1; Tapscript required for multisig and Lightning channel scripts in later phases |
| Ordinals inscribed on P2TR outputs cannot be detected without a separate indexer | accepted; chain service carries the accepted privacy trade-off for addresses; an inscription indexer is a Phase 4 addition |
| Lightning requires a parallel system; on-chain and Lightning BTC are separate balances | inherent to the Lightning protocol; unified balance UX is a Phase 5 design problem, not a Phase 1 one |

Do not describe this wallet as secure because tests pass. Document what remains broken.

**Testnet address collision:** testnet3, testnet4 and signet share identical address encoding (`tb1`). An address does not identify which of them it belongs to, and sending to the wrong one succeeds silently. Network is explicit state, never inferred, and is displayed next to every address. Regtest (`bcrt1`) and mainnet (`bc1`) are distinguishable. The signer has a test asserting this collision still holds, so it cannot be quietly forgotten.

**Cryptographic review:** the BIP implementations are `rust-bitcoin`, `bip39` and `libsecp256k1` — widely used, with years of bug reports behind them. The wiring around a reviewed library is still somewhere a bug can live (A.4).

---

## 10. Open decisions

- **Entropy source.** Nothing generates a recovery phrase yet. `mnemonic_from_entropy` takes bytes from the caller, deliberately, because where randomness comes from belongs at the edge of the system. On mobile that means the platform CSPRNG (`SecRandomCopyBytes` on iOS, `SecureRandom` on Android), and the decision is which one and how it is verified. **Blocks everything.**

- **Rewriting the network guard tests.** `signer/tests/mainnet_guard.rs` and `builder/tests/mainnet_guard.rs` assert the old §2. Until they assert the new one (no code path changes the network without a deliberate user action), mainnet must not be enabled.

- **Endpoint values.** The shape is settled — runtime config keyed by network, rejected at load if the key is not in the compiled catalogue. No provider URL has been verified against a live service, and §11 rejects invented APIs. **Blocks the chain service.**

- **Replace-by-fee.** Sequence is currently final. `0xFFFFFFFD` opt-in RBF required on all inputs before mainnet. **Blocks mainnet.**

- **Logging and crash reporting.** §5 forbids the phrase reaching logs. Which SDKs, if any, are permitted in a build that can hold a seed is undecided.

- **The approval screen's contents.** What is shown at the moment the biometric prompt appears, beyond the network name and amount.

- **P2TR pinned-address tests.** BIP-86 derivation paths and Bech32m encoding need pinned-vector tests for testnet4, signet, and mainnet before the signer is extended. Same standard as the existing P2WPKH tests. **Blocks Phase 1.1.**

- **Device-only flag test.** A test must fail if the Keychain/Keystore item is configured without the device-only flag. Not yet written. **Blocks mainnet.**

- **Compact block filters (BIP-158).** Would remove the service's view of every user's history. Large; not scheduled.

- **Chain service indexing strategy for Ordinals/Runes.** Third-party API (Hiro, Ordiscan) vs own `ord` indexer vs pluggable abstraction. Must be decided before Phase 4 begins; database is required either way.

- **Mainnet enablement.** A gate, not a pull request. Requires: external security review, a verified entropy source, the network guards rewritten, RBF implemented, device-only flag test passing, approval screen contents decided, logging decision recorded, and every "no answer yet" in §9 answered.

---

## 11. Working agreement

- Discussion and explicit permission precede implementation. No code is written ahead of an agreed step.
- Abstractions are extracted, not anticipated. No structure exists before something goes in it.
- A correctness or security problem stops forward progress.
- Placeholder security, fake chain responses in production paths, and invented APIs are rejected outright.
- Prefer a standard, well-reviewed library. Hand-written cryptography or encoding requires a recorded decision in Appendix A, not a preference.
- A test that cannot fail is deleted, not kept. A green suite that proves nothing is a false statement about coverage.
- A platform limitation is written down as a limitation. No control is described as stronger than it is.

---

## Appendix A. Decisions taken

Recorded because the reasoning outlasts the outcome, and because several of these reverse an earlier decision — those are the entries worth reading.

### A.1 The registry holds constants; endpoints are runtime config

The original plan was TypeScript-canonical, with Rust holding a verified copy of the constants and an agreement test between them.

The premise was wrong. Counting consumers showed the consensus constants have exactly one — the signer. The UI renders names and derives nothing; the chain service relays bytes to a URL. A service that validates an address is a service you are trusting to validate it correctly (§4), so it does not get them either. With one consumer there is no copy to keep in sync.

The constants/endpoints split came from the same exercise. Consensus constants never change and fail silently; endpoints change per deploy and fail loudly.

### A.2 Environment identifier is `chain:environment`

`bitcoin:testnet4` as the wire and persistence form, parsed into a typed value at every boundary so no code does string surgery on one.

### A.3 Bitcoin first, then Bitcoin only

Bitcoin was chosen first as the hardest of the three chains considered — UTXO selection, change outputs, vsize fees. Ethereum and Solana were subsequently dropped from scope entirely, along with §7's former cross-chain abstraction rules.

### A.4 Standard libraries, not hand-written BIP mechanics

**Reverses an earlier decision.** The first signer hand-rolled bech32, HMAC, PBKDF2, BIP-39 and BIP-32, on the reasoning that understanding was the success criterion. That was defensible while the only user was the author.

It shipped a real bug. BIP-39 requires NFKD normalisation of the mnemonic and passphrase. The hand-written version omitted it, so a passphrase containing a combining accent silently derived a **different wallet** from every other implementation — no error, and no way for a user to notice until their coins were somewhere unreachable. Found only by differential testing against the crates that now do the work.

**Swapping to the library did not fix the bug.** The first version of the swap called `Mnemonic::to_seed_normalized`, which means "the passphrase is *already* normalised" and skips the step — reintroducing the identical bug wearing a reviewed library's name. Caught by the regression test kept from the hand-rolled era. Libraries remove the burden of implementing a standard correctly, not of calling it correctly.

### A.5 The differential suite was deleted rather than ported

It compared the hand-written implementation against `rust-bitcoin`. Once `rust-bitcoin` became the implementation, those tests compared a library to itself: green forever, proving nothing. Replaced by tests of the wiring — pinned addresses, path assembly, network separation. §11's rule about tests that cannot fail comes from here.

### A.6 Native mobile, not React Native or web

One Rust core via UniFFI, with SwiftUI and Kotlin Compose above it. The cleanest binding, and direct access to Keychain/Secure Enclave and Keystore — which is the security upgrade that makes mainnet defensible at all. The browser's limitations (XSS on its own origin, malicious extensions reading the screen) have no answer; the phone's do.

Cost: two UI codebases and a Mac in the build chain. `registry/ui-catalog` (TypeScript) is retired — the catalogue comes through UniFFI from Rust instead, which removes the conformance check with it.

### A.7 Biometric only, no app passphrase

A password is something to forget, to phish, to reuse, and to write down next to the phone. Biometrics are enforced by the OS and cannot be typed into a fake screen. Recovery is the phrase either way, so a passphrase adds a failure mode without adding a recovery path.

### A.8 Seed encrypted in Rust, ciphertext in platform storage

Both layers share one trust root, so this is **not** two independent factors and should not be described as such. It buys two things: a Keychain misconfiguration leaks ciphertext rather than a seed, and the encrypted format is written once in Rust rather than twice in two platforms' idioms.

### A.9 Watch-only by default, per-signature unlock

Balances and receive addresses need only the account `xpub`. The seed is required for exactly one operation, so it is decrypted and wiped within that operation. No session window, no timeout to tune, and the biometric prompt appears precisely when the user is spending — which is when a deliberate confirmation is wanted anyway.

### A.10 Server-side chain data for v1

The app tells the chain service its addresses. Fast, light on battery and data, and simple. The service consequently sees every user's full history, which is stated in §6 rather than implied away. BIP-158 is the fix and is not scheduled.

### A.11 One app with a network switch

Rejected two separate builds, which would have made a testnet mistake structurally unable to touch mainnet funds. One app is more convenient for developers using Thadam as a test workspace, and it is the riskier choice, so §2's four obligations are mandatory rather than advisory — in particular that nothing external can change the network, and that the OS-drawn biometric prompt names it at the signing moment.

### A.12 QR preferred, paste confirmed

Clipboard hijacking is the most common real-world Bitcoin theft. Scanning is immune to it and is the default path. Paste stays available because people receive addresses in messages, and blocking it would push them to retype by hand — a different error with the same outcome — but a pasted address must be confirmed in full first.

### A.13 Written phrase only; no cloud backup

Lost phrases are the largest single cause of lost Bitcoin, and an encrypted backup to the user's own iCloud or Drive would not make Thadam custodial — it is their storage, not ours. It was still rejected: the backup password becomes the real key, and it is weaker than the phrase it protects. Some users will lose funds. That is the honest consequence of a non-custodial wallet.

### A.14 Fee sanity bounds

The fee rate is the only path from a lying chain service to lost user funds. A hardcoded plausible range plus explicit confirmation for anything unusual closes it for a few lines. Everything else the service can do — withheld coins, false confirmations — is documented as an accepted dependency rather than engineered around.

### A.15 P2TR (Taproot) is the default address type

P2TR (BIP-86, Bech32m, `bc1p`/`tb1p`) is the default for all new Thadam wallets. P2WPKH (BIP-84, Bech32, `bc1q`/`tb1q`) is supported for receiving and for compatibility with senders on older wallets, and is never generated as a default.

**Rationale:** Ordinals, Runes, Silent Payments (BIP-352), and all serious Bitcoin L2 development targets Taproot. Starting with P2WPKH as default and switching later requires migrating user address chains, which is a support and UX failure. Taproot key-path spends are also cheaper than P2WPKH (57.5 vbytes vs 68 vbytes per input), so users on the default path pay lower fees.

**What this requires of the codebase:**
- The signer generates BIP-86 paths by default; BIP-84 paths are explicitly opt-in
- Address encoding uses Bech32m for P2TR; the two encoding schemes must never be mixed
- The builder calculates per-type input weights; a single hardcoded weight is rejected
- Change outputs are always P2TR regardless of what was spent
- Pinned-address tests cover both derivation paths on all supported networks

**What this does not yet include:** Tapscript (script-path spend). Key-path spend covers the standard single-sig use case. Script-path spend is required for multisig and Lightning channel scripts and is deferred to later phases.
