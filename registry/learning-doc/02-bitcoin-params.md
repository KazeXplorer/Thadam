# BitcoinParams — What Each Field Is and When It Matters

This doc explains the seven constants in `BitcoinParams`. Not how the full
pipelines (key derivation, address encoding, signing) work — that comes when
we build those modules. Just: what is this field, what consumes it, and why
is it in the registry.

---

## The Struct at a Glance

```rust
pub struct BitcoinParams {
    pub bech32_hrp: &'static str,          // "tb" or "bcrt"
    pub base58_p2pkh_version: u8,          // 0x6F
    pub base58_p2sh_version: u8,           // 0xC4
    pub base58_wif_version: u8,            // 0xEF
    pub bip32_public_version: u32,         // 0x0435_87CF
    pub bip32_private_version: u32,        // 0x0435_8394
    pub bip44_coin_type: u32,              // 1
}
```

Every field is a **network-specific constant** — a value that differs between
mainnet and testnet. These constants are consumed by different parts of the
wallet at different times. None of them change after the network is created.
They come from Bitcoin Core's source code (`src/kernel/chainparams.cpp`),
BIP standards, and SLIP-0044.

---

## 1. `bech32_hrp` — Human-Readable Part

**Value:** `"tb"` (testnet4, signet) or `"bcrt"` (regtest)

**What it is:** The prefix of a modern Bitcoin address. When you see an address
like `tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx`, the `tb` at the front is
the HRP.

**When you'll use it:** When Thadam builds the address module. The flow will be
roughly: take a public key → hash it → encode it with bech32/bech32m using
this HRP → out comes a human-readable address. The HRP is the first thing a
human or software reads to know "this address belongs to testnet."

**The hazard your registry documents:** Testnet4 and signet both use `"tb"`.
That means you CANNOT tell from an address alone which of those two networks
it belongs to. This is why the registry forces you to always carry the network
as explicit state alongside the address — never infer it from the address string.
Regtest uses `"bcrt"`, so it's the only test network where the address self-
identifies.

**Mainnet equivalent (which Thadam will never hold):** `"bc"` — as in
`bc1q...`. The `no_mainnet.rs` test asserts no network in the registry uses this.

---

## 2. `base58_p2pkh_version` — Pay-to-Public-Key-Hash Version Byte

**Value:** `0x6F` (testnet)

**What it is:** A single byte prepended to a public key hash before base58check
encoding. It determines the first character of a legacy address:
- `0x6F` → address starts with `m` or `n` (testnet)
- `0x00` → address starts with `1` (mainnet — forbidden in Thadam)

**When you'll use it:** Only if Thadam ever supports legacy (non-SegWit)
addresses. For a testnet-only wallet in 2026, you'll almost certainly use
bech32m (SegWit v1/Taproot) addresses exclusively, making this field
informational rather than operational. It's in the registry because it's part
of the network's identity — a complete description of a Bitcoin network
includes it.

**What "P2PKH" means in one sentence:** "Send coins to whoever can prove they
own the private key behind this public key hash." It's the oldest address type.

---

## 3. `base58_p2sh_version` — Pay-to-Script-Hash Version Byte

**Value:** `0xC4` (testnet)

**What it is:** Same idea as P2PKH version, but for script addresses:
- `0xC4` → address starts with `2` (testnet)
- `0x05` → address starts with `3` (mainnet)

**When you'll use it:** Probably never directly. P2SH is used for wrapped-SegWit
and multisig. A modern wallet that goes straight to native SegWit (bech32m)
doesn't need P2SH encoding. Again, it's here for completeness.

**What "P2SH" means in one sentence:** "Send coins to whoever can provide a
script that hashes to this value and satisfies that script." More flexible
than P2PKH — it's how multisig and complex conditions work in legacy Bitcoin.

---

## 4. `base58_wif_version` — Wallet Import Format Version Byte

**Value:** `0xEF` (testnet)

**What it is:** A byte prepended to a raw private key before base58check
encoding, producing a WIF string. WIF is how private keys are represented as
human-readable strings for export/import between wallets:
- `0xEF` → WIF starts with `c` (testnet, compressed key)
- `0x80` → WIF starts with `K` or `L` (mainnet, compressed)

**When you'll use it:** If Thadam ever supports exporting or importing private
keys as text. This is a "copy your private key to move to another wallet"
feature. For a non-custodial wallet that derives keys from a seed phrase
(BIP-39 → BIP-32), WIF export is optional — most modern wallets use the seed
phrase as the backup mechanism instead.

**Why it matters for safety:** If you accidentally use `0x80` here, an exported
key looks like a mainnet key. Another wallet importing it might try to use it
on mainnet. The version byte is a signal to other software about which network
the key belongs to.

---

## 5. `bip32_public_version` — Extended Public Key Version Bytes

**Value:** `0x0435_87CF` (testnet → `tpub...`)

**What it is:** Four bytes at the start of a serialized extended public key
(xpub). They determine the human-readable prefix:
- `0x0435_87CF` → `tpub` (testnet)
- `0x0488_B21E` → `xpub` (mainnet)

**What an extended public key is (brief):** A public key plus a "chain code"
that together allow deriving child public keys without the private key. This
is how a watch-only wallet works — it has the `tpub` and can generate all
your addresses to show balances, but can't sign transactions.

**When you'll use it:** When Thadam implements BIP-32 HD (hierarchical
deterministic) key derivation. The extended key is the root from which all
addresses in the wallet are generated. The version bytes go into the
serialized form so that any software reading the key knows it's a testnet key.

---

## 6. `bip32_private_version` — Extended Private Key Version Bytes

**Value:** `0x0435_8394` (testnet → `tprv...`)

**What it is:** Same as above but for the private half:
- `0x0435_8394` → `tprv` (testnet)
- `0x0488_ADE4` → `xprv` (mainnet)

**When you'll use it:** Same moment as the public version — during HD key
derivation. The `tprv` is the master secret from which everything else is
derived. In a running wallet, this value lives in memory only as long as
needed for signing, then is zeroized. The version bytes are a safeguard —
software seeing `tprv` knows not to broadcast transactions to mainnet.

---

## 7. `bip44_coin_type` — SLIP-0044 Coin Type

**Value:** `1` (all test networks)

**What it is:** The second level of a BIP-44 derivation path:

```
m / purpose' / coin_type' / account' / change / index
m / 44'      / 1'          / 0'       / 0      / 0
```

- `1` = "Testnet (all coins)" — every Bitcoin test network and every altcoin
  testnet uses this same value
- `0` = Bitcoin mainnet (forbidden in Thadam)

**When you'll use it:** When deriving addresses. The derivation path is the
"route" from the master seed to a specific address. The coin type slot tells
the derivation "this tree of keys is for testnet." Using `1` ensures that
even if someone imports the same seed phrase into a mainnet wallet, the
mainnet wallet derives keys from the `coin_type = 0` branch and never sees
the testnet addresses (and vice versa). It's key-space separation by
convention.

**Why all test networks share `1`:** SLIP-0044 doesn't distinguish between
testnet4 and signet — they're all "testnet." This means the same seed phrase
on testnet4 and signet produces the same addresses (which, combined with the
shared `bech32_hrp`, means the addresses are literally identical strings).
This is the collision hazard the registry documents.

---

## How These Fields Group by When You'll Need Them

| When | Fields | What's happening |
|---|---|---|
| **Address derivation** (building next) | `bip44_coin_type`, `bip32_*_version`, `bech32_hrp` | Seed → master key → derived key → address |
| **Displaying addresses** | `bech32_hrp` | Encoding a public key hash into a readable string |
| **Key export/import** | `base58_wif_version` | Encoding a private key for human transfer |
| **Legacy address support** | `base58_p2pkh_version`, `base58_p2sh_version` | Only if Thadam supports pre-SegWit addresses |

For Thadam's initial build (testnet-only, modern SegWit), the fields you'll
touch first are `bip44_coin_type`, `bip32_private_version`, and `bech32_hrp`.
The base58 fields exist for completeness and may never be operationally used.

---

## One Mental Model

Think of `BitcoinParams` as a **network's fingerprint**. Every field answers
the same question from a different angle: "which network does this thing
belong to?" An address says it with the HRP. A private key says it with the
WIF version. A derivation path says it with the coin type. An extended key
says it with the version bytes.

The reason there are so many ways to say the same thing is historical —
Bitcoin added features over 15 years, and each feature needed its own way to
tag "which network." A modern wallet that only uses bech32m addresses could
technically get by with just the HRP and the coin type. But the registry
holds all of them because:

1. Libraries you'll depend on expect them (the `bitcoin` Rust crate's
   `Network` type carries all of these).
2. A "complete" network definition is future-proof — if Thadam later supports
   legacy imports or watch-only xpubs, the constants are already there.
3. The `no_mainnet.rs` test checks ALL of them. If the registry only held
   the HRP, someone could accidentally put a mainnet WIF version byte in a
   library config somewhere and the registry's safety net wouldn't catch it.
