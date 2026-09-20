# Thadam — Solflare for Bitcoin: Phased Roadmap

**Vision:** The power-user, non-custodial Bitcoin wallet. Deep ecosystem integration — on-chain, Taproot-native, Ordinals/Runes-aware, Lightning in v2. Built for developers, testers, and serious holders globally.

**Contrarian bet:** Bitcoin's ecosystem (Taproot, Ordinals, Runes, BitVM, Ark) will expand significantly in 3–5 years. No dominant power-user mobile wallet is positioned to capture it. Thadam is built to be that wallet.

This document is a roadmap and feature list. Architecture constraints and decisions live in `README.md`. Read that first.

---

## Phase 1 — Testnet Foundation
**Networks:** `bitcoin:testnet4`, `bitcoin:signet`
**Goal:** A working, secure, installable wallet on both platforms. No mainnet. No ecosystem features. Correctness and security only.

### 1.0 — Close all §10 blockers (pre-mobile prerequisite)
These block everything. No mobile code until all three are resolved.

- [ ] **Entropy source** — platform CSPRNG (SecRandomCopyBytes on iOS, SecureRandom on Android) called from the mobile layer, bytes passed into `mnemonic_from_entropy`. Verified by a test that checks output distribution and rejects a zero-filled buffer.
- [ ] **Network guard tests rewritten** — `signer/tests/mainnet_guard.rs` and `builder/tests/mainnet_guard.rs` now assert that no code path changes the network without a deliberate user action, replacing the old assertion that mainnet is absent.
- [ ] **Replace-by-fee (RBF)** — sequence field set to `0xFFFFFFFD` on all inputs. Required before any transaction is trustworthy in a congested mempool. Tested: a built transaction must have the opt-in sequence value; a transaction without it must be rejected.

### 1.1 — Address type foundation (P2TR default, P2WPKH supported)
Decided: P2TR (BIP-86, `bc1p` / `tb1p`) is the default for all new wallets. P2WPKH (BIP-84, `bc1q` / `tb1q`) is supported for receiving from legacy wallets and for change where needed.

**Signer changes:**
- [ ] Add BIP-86 derivation path (`m/86'/coin_type'/account'/0/index`) alongside existing BIP-84
- [ ] Add P2TR address encoding (Bech32m, not Bech32) — via `rust-bitcoin`
- [ ] Pinned address tests for both types on testnet4 and signet
- [ ] Network separation tests: a P2TR address on testnet4 must not be accepted as signet, and vice versa

**Builder changes:**
- [ ] UTXO input handling for P2TR (key-path spend, Schnorr signature, annex-free)
- [ ] Weight/vsize calculation updated — P2TR inputs are 57.5 vbytes (vs 68 for P2WPKH); fee estimation must use correct per-type weights
- [ ] Mixed input support: a transaction may spend P2WPKH and P2TR UTXOs together; each input's weight is calculated independently
- [ ] Change output defaults to P2TR

**Chain service changes:**
- [ ] Index P2TR addresses alongside P2WPKH; both prefixes (`tb1p`, `tb1q`) must be tracked per wallet

**Decision to record in README Appendix A:**
> A.15: P2TR is the default address type for all new Thadam wallets. Rationale: Ordinals, Runes, Silent Payments, and all serious Bitcoin L2 development targets Taproot. Starting with P2WPKH default and switching later requires migrating user addresses — a support and UX failure. P2WPKH is supported for receiving and for compatibility, never as the default for new address generation.

### 1.2 — Mobile layer: iOS (SwiftUI) + Android (Kotlin Compose)
**Screens:**
- [ ] Onboarding — create wallet (entropy → mnemonic display → word verification) or import (phrase entry)
- [ ] Home — balance (sats + fiat), network badge (TESTNET4 / SIGNET in text, not colour alone), recent transactions
- [ ] Receive — P2TR address by default, QR, copy button, address in full (no truncation)
- [ ] Send — amount, recipient (QR scan default, paste with confirmation screen), fee tier selector (slow / standard / fast from mempool), review screen, biometric prompt
- [ ] Transaction detail — txid, amount, fee, confirmations, network, addresses in full
- [ ] Settings — network switch (testnet4 ↔ signet), address type display, show recovery phrase (biometric gated)

**Biometric prompt wording (OS-drawn):**
> "Send [amount] BTC on [NETWORK]"

**UI rules carried from architecture:**
- Every money screen names its network in words
- Addresses displayed in full, grouped (never `tb1q…8f3t`)
- App-switcher preview blurred on phrase and send screens
- Paste requires full-address confirmation screen

### 1.3 — Wallet session state machine
```
Empty → Draft → Reviewed → Signed → Broadcast
```
- [ ] Implemented in Rust, exercised by a test suite with no app present
- [ ] Illegal transitions are absent, not guarded
- [ ] The full send flow driven from Rust tests before the UI is connected

### 1.4 — Chain service: live testnet4 + signet
- [ ] Endpoint values verified against live providers (Blockstream Esplora + mempool.space — free, no key required for testnet)
- [ ] Fee estimation: three tiers (slow / standard / fast) from mempool fee histogram
- [ ] Fee sanity bounds enforced in-app (hardcoded plausible range; explicit confirmation for anything outside it)
- [ ] UTXO fetch, balance calculation, transaction broadcast, confirmation polling

### 1.5 — Revenue: fiat on-ramp integration
First revenue stream. Integrated in Phase 1 even at zero users — validates the flow before mainnet.

- [ ] MoonPay widget embedded as "Buy BTC" in the app (no custody, no KYC burden on Thadam)
- [ ] Partner programme signed — revenue share agreed before launch
- [ ] On-ramp flow tested on testnet (MoonPay sandbox environment)
- [ ] Off-ramp ("Sell BTC") integrated alongside — same partner, same widget

**Phase 1 exit criteria:**
- App installable on TestFlight and Play Store internal track
- Full send/receive cycle on testnet4 and signet
- Both address types (P2TR, P2WPKH) generating and receiving correctly
- All §10 blockers closed
- No mainnet code path reachable from the UI
- MoonPay on-ramp/off-ramp integrated and tested in sandbox
- Chain service hosted and live against testnet4 + signet public endpoints
- Monthly infrastructure cost confirmed at or below $15/month

---

## Phase 2 — Mainnet Gate
**Networks:** adds `bitcoin:mainnet`
**Goal:** not a sprint. A gate. Every condition must be met before mainnet is enabled.

### Gate conditions (all mandatory)
- [ ] **External security audit** — at minimum `signer/` and `builder/`. No shortcuts. Thadam's own code is unaudited (§9); that is the highest-consequence open item.
- [ ] **Device-only flag test written and passing** — a test must fail if the Keychain/Keystore item is configured without the device-only flag.
- [ ] **Approval screen contents decided** — what appears in the OS biometric prompt beyond network name (§10). Decided and implemented.
- [ ] **Logging/crash SDK decision** — which SDKs, if any, are permitted in a build that can hold a seed (§10). Recorded in Appendix A.
- [ ] **Mainnet endpoints verified** — live provider URLs confirmed working against mainnet.
- [ ] **All §9 "no answer yet" items answered.**

- [ ] **RPC provider for mainnet decided and verified** — GetBlock free tier ($0) for early mainnet, QuickNode ($49/month) when rate limits are hit, or self-hosted Bitcoin full node (~€15/month on Hetzner) at scale. Decision recorded in README §10 before mainnet is enabled.
- [ ] **MoonPay partner programme live on mainnet** — sandbox tested in Phase 1; mainnet revenue share active at Phase 2 launch.

**Phase 2 deliverable:** same app, mainnet enabled, gate passed. No new features — just the gate. Expected infrastructure cost: $35–60/month.

---

## Phase 3 — Power-User On-Chain Features
**Networks:** mainnet + testnet4 + signet
**Goal:** Sparrow-level on-chain power, on mobile. This is where Thadam becomes a tool, not just a wallet.

### 3.1 — UTXO coin control
- [ ] UTXO list view — each coin: amount, confirmations, address, txid
- [ ] Manual coin selection for a send — override the automatic branch-and-bound selection
- [ ] UTXO labels — user-defined notes per coin (stored locally, never transmitted)
- [ ] Freeze/lock a UTXO — excluded from automatic coin selection (critical prerequisite for Ordinals: inscribed sats must not be accidentally spent)

### 3.2 — Transaction management
- [ ] Full transaction history with user-defined labels
- [ ] RBF fee bump — detect opt-in RBF transactions, allow fee increase on pending sends
- [ ] CPFP — detect stuck incoming transactions, offer to create a child transaction with higher fee
- [ ] Custom fee input (sat/vB) with mempool fee histogram context

### 3.3 — Multi-account
- [ ] Multiple named accounts under one seed (BIP-44 account index)
- [ ] Each account has its own P2TR (default) and P2WPKH address chain
- [ ] Account switcher in UI

### 3.4 — Watch-only wallets
- [ ] Import an xpub — monitor balance and history without private key
- [ ] Watch-only accounts clearly marked; no signing prompt ever shown for them

### 3.5 — Address book
- [ ] Named contacts with Bitcoin addresses
- [ ] Contact selected from address book populates send recipient
- [ ] Contacts stored locally only

### 3.6 — Hardware wallet support (Ledger)
- [ ] Define the signer port interface (noted as premature in §3 until this phase)
- [ ] Ledger Bitcoin app integration — PSBT signing over USB/BLE
- [ ] The signer port takes a structured payload (PSBT), never a bare hash — rule already in architecture
- [ ] Hardware wallet accounts displayed distinctly in UI
- [ ] Ledger affiliate link in settings — "Don't have a hardware wallet?" → Ledger affiliate programme, fixed referral fee per purchase

**Phase 3 exit criteria:**
- Full UTXO control including freeze (prerequisite for Phase 4)
- RBF and CPFP working on mainnet
- Multi-account and watch-only functional
- Ledger signing demonstrated end-to-end

---

## Phase 4 — Ecosystem: Ordinals + Runes
**Networks:** mainnet (testnet4 for development)
**Goal:** Bitcoin ecosystem wallet. Thadam understands what its UTXOs contain, not just their BTC value.

### 4.0 — Chain service indexing strategy (decided before Phase 4 begins)
Third-party API first (Hiro or Ordiscan), own `ord` indexer later if needed. A pluggable provider interface in the chain service abstracts the difference. Database required — inscription ownership and Rune balances are stateful and cannot be re-derived from UTXOs alone.

**Infrastructure added:** PostgreSQL database ($10–25/month). Hiro/Ordiscan API free tier → paid ($50–200/month at scale). Own `ord` indexer on self-hosted node eliminates API cost if already running a full node from Phase 2.

**Decision to record in Appendix A before Phase 4 begins.**

### 4.0b — Revenue: Ordinals marketplace referral
- [ ] Integration with Magic Eden or Ordiscan marketplace — "Sell" CTA on each inscription deep-links to marketplace with referral parameter
- [ ] Referral revenue share negotiated with marketplace partner before Phase 4 launch
- [ ] No custody or order-book involvement — pure referral routing

### 4.1 — Ordinals / Inscriptions
- [ ] Detect inscribed UTXOs in the wallet's coin set
- [ ] Display inscriptions: image/text content, inscription number, sat offset
- [ ] Freeze inscribed UTXOs by default — they must not be accidentally spent as fee or change
- [ ] Explicit user action required to spend an inscribed UTXO
- [ ] Receive flow: incoming inscriptions shown distinctly from plain BTC
- [ ] Send inscription: select inscribed UTXO, recipient, fee — builder must not add it to change or fee outputs

### 4.2 — Runes
- [ ] Rune balance display per wallet and per account
- [ ] Rune transfer: select Rune, amount, recipient — builder constructs the OP_RETURN runestone correctly
- [ ] Rune receive: detect incoming Rune allocations
- [ ] Runes and BTC balance shown separately; no conflation
- [ ] In-app Rune swap (when DEX liquidity matures) — 0.3–0.5% convenience fee on top of market rate; highest revenue ceiling in the ecosystem tier

### 4.3 — Silent Payments (BIP-352)
- [ ] Generate a Silent Payment address (reusable, privacy-preserving)
- [ ] Scan for incoming Silent Payments (requires UTXO set scanning — chain service extension)
- [ ] Send to a Silent Payment address

**Phase 4 exit criteria:**
- No inscribed sat can be accidentally spent
- Rune balances accurate and transfers functional
- Silent Payment address generation and scanning working

---

## Phase 5 — Lightning Network
**Goal:** A second payment system sharing the seed root. Designed from scratch when Phase 2 is complete. Not a feature addition — a parallel product track.

### Design decisions deferred to Phase 5 design session:
- LSP selection (Phoenix LSP, Greenlight, Voltage, or self-hosted)
- Channel lifecycle UX (open, close, force close)
- Inbound liquidity UX (new users cannot receive without a channel)
- Unified balance UX: how on-chain and Lightning BTC are shown together without confusing users
- LNURL support (pay, withdraw, auth)
- Lightning state machine (separate from `Empty → Draft → Reviewed → Signed → Broadcast`)
- `bitcoin:lightning` as the environment identifier for the Lightning wallet

**Phase 5 entry condition:** Phase 2 (mainnet) complete and stable.

---

## Architectural additions required per phase

| Addition | Phase | File/Layer |
|---|---|---|
| BIP-86 derivation + P2TR encoding | 1.1 | `signer/` |
| P2TR input weight in fee calc | 1.1 | `builder/` |
| Bech32m address encoding | 1.1 | `signer/` |
| Mobile layer (SwiftUI + Compose) | 1.2 | `ios/`, `android/` |
| Wallet session state machine | 1.3 | `session/` (new crate) |
| Live chain service endpoints | 1.4 | `chainservice/` |
| Device-only flag test | 2 | `ios/tests/`, `android/tests/` |
| UTXO labels + freeze (local storage) | 3.1 | `ios/`, `android/` |
| RBF fee bump + CPFP | 3.2 | `builder/`, `session/` |
| xpub import (watch-only) | 3.4 | `signer/` |
| Signer port interface | 3.6 | `signer/` |
| Chain service database | 4.0 | `chainservice/` |
| Ordinals indexer integration | 4.1 | `chainservice/` |
| Runestone builder | 4.2 | `builder/` |
| Silent Payment scanning | 4.3 | `chainservice/`, `signer/` |
| Lightning state machine + LSP | 5 | new crate `lightning/` |

---

## Revenue Model

Self-sovereign architecture rules out user accounts and server-side subscription state. Revenue comes from transactional and referral flows only — no subscription, no custody, no user data.

### By Phase

| Stream | Phase available | How it works | Your cut |
|---|---|---|---|
| **Fiat on-ramp fee share** | Phase 1 | Integrate MoonPay/Transak widget as "Buy BTC" button; they handle KYC and banking; you earn per transaction | 0.5–1% of transaction value |
| **Ledger affiliate** | Phase 3 | "Get a hardware wallet" link in settings; Ledger affiliate programme pays per referral | Fixed per-unit, small |
| **Ordinals marketplace referral** | Phase 4 | "Sell" on an inscription routes to Magic Eden/Ordiscan; they pay referral cut per trade | Negotiated per partner |
| **Rune in-app swaps** | Phase 4+ | Thin convenience fee on top of market rate when Bitcoin DEX liquidity matures | 0.3–0.5% per swap |
| **Lightning LSP referral** | Phase 5 | LSP pays referral fee per channel opened through Thadam | Negotiated per LSP |

### Priority order
1. **Fiat on-ramp** — only realistic revenue before Phase 4. Integrate in Phase 1, even at zero users, to validate the flow.
2. **Rune swaps** — highest ceiling if the Bitcoin ecosystem grows as expected. Phase 4+.
3. **Everything else** — marginal individually; meaningful at user scale.

### What to integrate first
MoonPay is the default first call — 500+ wallet partners, widest fiat coverage, established revenue-sharing programme. Transak and Ramp are alternatives with similar models. All three embed as a widget inside the app; your chain service is not involved.

### What subscription would look like if revisited
No user accounts means no web-based subscription escape hatch (the browser extension workaround requires account state). The only viable path is Apple StoreKit 2 / Google Play Billing — Apple/Google take 15–30%. Decision: deferred indefinitely. On-ramp and ecosystem revenue are the primary model.

---

## Infrastructure Costs

### Phase 1 — Testnet only

| Item | Provider | Cost |
|---|---|---|
| Go chain service hosting | Railway / Render / Fly.io | $5–10/month |
| Testnet4 + signet RPC | Blockstream Esplora + mempool.space (public) | **$0** |
| Apple Developer account | Apple | $99/year (~$8/month) |
| Google Play account | Google | $25 one-time |
| **Phase 1 total** | | **~$15/month + $124 one-time** |

Blockstream Esplora and mempool.space public APIs are free and sufficient for testnet volumes. No RPC spend until mainnet.

### Phase 2 — Mainnet added

| Item | Provider | Cost |
|---|---|---|
| Go chain service hosting | Hetzner VPS (2GB RAM) | €6/month |
| Mainnet RPC — low volume | GetBlock (40K req/day free) → paid at scale | $0 → $30/month |
| Mainnet RPC — reliable SLA | QuickNode | $49–$299/month |
| mempool.space fee data | Sponsorship tier | ~$500/month at scale; public free tier until then |
| **Phase 2 total (early)** | | **~$35–60/month** |

### Phase 3+ — Scale decision point

At meaningful user scale, the public RPC providers rate-limit you. Two options:

| Option | Cost | Trade-off |
|---|---|---|
| Dedicated provider (QuickNode/GetBlock) | $100–300/month | No ops burden; provider is a reliability dependency |
| Self-hosted Bitcoin full node | ~€15–20/month (Hetzner AX41, 600GB disk) | Full control; you own the ops; ~600GB storage, ~4GB RAM |

Self-hosting a node is cheaper at scale and removes the provider dependency risk stated in §6. Worth recording as an open decision in README §10 before Phase 2 begins.

### Phase 4 — Ordinals/Runes indexing added

| Item | Provider | Cost |
|---|---|---|
| Ordinals API (Phase 4 start) | Hiro API / Ordiscan | Free tier → $50–200/month at scale |
| Chain service database | PostgreSQL on Hetzner or Supabase | $10–25/month |
| **Phase 4 additional** | | **~$60–225/month** |

Own `ord` indexer (self-hosted) is the long-term path if third-party APIs become a reliability or cost problem. Requires ~600GB additional disk alongside the Bitcoin node.

### Summary across phases

| Phase | Monthly cost (early) | Monthly cost (at scale) |
|---|---|---|
| Phase 1 (testnet) | ~$15 | ~$15 |
| Phase 2 (mainnet) | ~$35–60 | ~$100–300 |
| Phase 3 (power features) | ~$35–60 | ~$100–300 |
| Phase 4 (ecosystem) | ~$95–285 | ~$200–500 |
| Phase 5 (Lightning) | TBD at design time | TBD |

Phase 1 and early Phase 2 are self-fundable. Phase 4 at scale needs on-ramp or ecosystem revenue to cover costs — which is intentional: by Phase 4, you should have enough users for on-ramp revenue to matter.

---

## What this is not

- A timeline. Phases have exit criteria, not dates.
- A complete feature list. Features not listed here require a discussion before implementation (§11 of `README.md`).
- Permission to skip a phase. Phase 2's gate is a gate, not a suggestion.
- A revenue guarantee. On-ramp revenue requires user scale. Ecosystem revenue requires Bitcoin's ecosystem to grow. Both are bets, stated plainly.
