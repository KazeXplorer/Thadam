# Builder

Coins and intent in, unsigned transaction and review summary out.
ARCHITECTURE.md §3: **deterministic, no network access, no key access.**

```
builder/
  src/
    lib.rs      assembly: target, selection, change, transaction
    select.rs   branch and bound, largest-first fallback
    fee.rs      sizes, effective values, cost of change, dust
    review.rs   §4 — the summary, derived from bytes
    utxo.rs     coins and intent
  tests/
    builder.rs        properties over random inputs
    review.rs         §4 under tampering
    network_guard.rs  §2 and §8 for this crate
```

## The three constraints are load-bearing

- **No keys.** This crate sits above the signer and nothing in it accepts a
  private key. It cannot sign even by mistake.
- **No network.** A transaction can be built and reviewed offline, and the
  review cannot be influenced by anything arriving mid-flight.
- **Deterministic.** Same coins, same intent, same transaction — every time.
  Without this, §4's review describes something that might differ from what
  gets signed a moment later. This is the one place this crate deliberately
  departs from Bitcoin Core, which shuffles its candidates.

## Coin selection

The part of a Bitcoin wallet with no equivalent in an account-model chain.
Ethereum has a balance you subtract from; Bitcoin has discrete coins, and paying
someone means choosing which to destroy and what to give yourself back.

**Branch and bound** runs first, looking for a subset that pays the target with
*no change at all*. Changeless transactions are smaller, cheaper, leave no coin
to manage, and tell a chain observer nothing about what you hold. It searches
for a subset landing in `[target, target + cost_of_change]` — anything in that
window is cheaper than making change.

**Largest first** runs when no such subset exists, which is most of the time.

Both work in **effective values**: each coin's own input fee is subtracted up
front. That breaks the circularity where the fee depends on how many coins are
selected and how many are selected depends on the fee. It also makes something
visible that face values hide — a 300-satoshi coin at a high fee rate is worth
less than nothing, and spending it makes you poorer. Those are dropped before
the search begins, which is why "you have a balance but cannot spend it" is a
distinct error here rather than "insufficient funds".

### Cost of change is two fees, not one

The bytes to create a change output now, *and* the bytes to spend it in some
future transaction. Counting only the first is how wallets accumulate dust they
can never economically spend.

## §4: the review is derived from bytes

`ReviewSummary::derive` takes a transaction, its input values, the network, and
the change address. **It does not take an `Intent`.** There is no parameter to
pass one through, so summarising the request instead of the payload is not a
discipline someone has to remember — it is not expressible.

This matters because the two can differ, and every way they differ is a way a
user loses money. A summary generated from intent would render a correct-looking
approval screen for a transaction that pays somewhere else.

The tampering tests are deliberately **value-conserving**: they move satoshis
from change to an attacker without touching the totals or the fee. A review that
only sanity-checked the fee would see nothing wrong. What changes is how much
leaves the wallet, which is the number that matters.

One property worth knowing: for an all-segwit transaction the txid does not
cover the witness, so the txid shown at review is the txid that lands on chain.
What the user approves is findable afterwards.

## What the tests found

**A real bug, caught by a test written in response to a mutation test.** When
branch-and-bound found a changeless selection, `build` ignored that and
recomputed a change amount from scratch — finding it above dust and adding back
the very output the search existed to avoid. Larger transaction, higher fee, and
a change coin to manage, on the code path specifically designed to produce none.
The fix makes the change decision follow the strategy rather than recompute.

The test that caught it did not exist an hour earlier. It was written because
mutating `cost_of_change` to drop its future-spend half survived the entire
suite, which said the window width was untested.

### Mutation results

| mutation | caught by |
|---|---|
| fee priced for 1 output when there are 2 | fee-rate and dust-change properties |
| change flag always false | change recognition, recipient amount |
| `change >= dust` → `> dust` | dust-boundary test *(added after it survived)* |
| `cost_of_change` drops the future spend | window-width test *(added after it survived)* |
| branch-and-bound result gets change anyway | window-width test *(the bug above)* |

One mutation in the first round was weak rather than uncaught: removing the
`effective_value > 0` filter changes almost nothing, because `checked_sub`
already drops coins worth less than their input fee.

## Properties checked over random inputs

Fixed seed, so a failure is reproducible from the test output alone.

| property | why it is the one that matters |
|---|---|
| inputs = outputs + fee | Bitcoin never records the fee; it is the leftover. An arithmetic slip hands a miner the difference silently |
| requested fee rate always met or exceeded | underpaying leaves a transaction stuck with no cancel button |
| no output is ever dust | the network drops it; the wallet sees a perfectly valid transaction that never confirms |
| recipient receives the exact amount | fees come out of change, never out of the payment |
| building twice is identical | §3 |

## Known gaps

- **One recipient.** Batching is a different shape; §10 says no structure before
  something goes in it.
- **No replace-by-fee.** Sequence is final. RBF is a real feature and a
  deliberate decision, not a default to drift into.
- **Lock time is zero.** Setting it to the current height is standard
  anti-fee-sniping, and it needs chain state that §3 forbids this layer from
  having. It belongs wherever the chain service's data arrives.
- **P2WPKH only.** Every size estimate assumes it, because that is all §5's
  BIP-84 derivation produces. Taproot would need its own predictions.
- **No fee-rate discovery.** The rate is an input. Finding a sensible one needs
  the mempool, which needs the chain service.
- **Largest-first is not privacy-optimal.** It consolidates a wallet into fewer,
  larger coins, which keeps future transactions small and tells an observer more.
  Single random draw is the usual alternative and is not implemented.
