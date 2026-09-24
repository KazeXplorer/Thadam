package bitcoin

import "context"

// UTXO is one unspent output belonging to an address.
//
// The chain service is a relay — it returns the data the provider gives it
// without re-validating amounts or scripts. The Rust builder on the client
// side is the authority on what the transaction costs; the service's job is
// to hand over the raw material.
type UTXO struct {
	// Txid is the transaction ID of the transaction that created this output,
	// as a hex string (64 characters, big-endian byte order).
	Txid string `json:"txid"`

	// Vout is the output index within that transaction.
	Vout uint32 `json:"vout"`

	// Value is the output's value in satoshis.
	Value uint64 `json:"value"`

	// Confirmed is true when the transaction containing this output has at
	// least one confirmation. Unconfirmed UTXOs are included in the response
	// because the wallet may want to show a pending balance, but the builder
	// should not spend them for a send — it cannot know whether they will
	// confirm, and spending them requires RBF support (§10).
	Confirmed bool `json:"confirmed"`

	// BlockHeight is the block height at which the output was confirmed.
	// Zero when Confirmed is false.
	BlockHeight uint32 `json:"block_height,omitempty"`
}

// BroadcastResult is the outcome of a broadcast attempt.
type BroadcastResult struct {
	// Txid is the transaction ID assigned by the network, as a hex string.
	// Present only when the broadcast succeeded.
	Txid string `json:"txid,omitempty"`
}

// Provider is what the chain service needs from a Bitcoin data source.
//
// The interface is deliberately narrow: the service is a relay (§3 of
// ARCHITECTURE.md) and the only operations it needs are the ones the mobile
// app will ask for. Nothing here implies a Bitcoin node — every method maps
// cleanly onto a public Esplora REST endpoint.
//
// Implementations are expected to be safe for concurrent use from multiple
// goroutines.
type Provider interface {
	// UTXOs returns all unspent outputs for the given address.
	//
	// Both confirmed and unconfirmed UTXOs are returned. The caller is
	// responsible for deciding which to use. Returns an empty slice, not nil,
	// when the address has no UTXOs.
	UTXOs(ctx context.Context, address string) ([]UTXO, error)

	// FeeEstimates returns current fee rate estimates for several confirmation
	// targets. All rates have passed the A.14 sanity bounds.
	FeeEstimates(ctx context.Context) (FeeEstimates, error)

	// Broadcast submits a signed transaction to the network.
	//
	// rawTx is the raw transaction bytes, hex-encoded. The provider returns the
	// txid if the transaction was accepted; it returns an error if the
	// transaction was rejected. The chain service relays both outcomes to the
	// client without interpreting them.
	Broadcast(ctx context.Context, rawTx string) (BroadcastResult, error)

	// Transactions returns the transaction history for the given address,
	// including both confirmed and unconfirmed transactions, newest first.
	Transactions(ctx context.Context, address string) ([]Transaction, error)

	// GetTx returns a single transaction by its txid.
	//
	// Returns an error if the transaction is not found or the provider is
	// unavailable.
	GetTx(ctx context.Context, txid string) (Transaction, error)

	// BlockHeight returns the height of the current best block.
	//
	// Used by the mobile app to compute confirmation counts:
	//   confirmations = blockHeight - tx.Status.BlockHeight + 1
	BlockHeight(ctx context.Context) (uint32, error)

	// TransactionsSince returns confirmed transactions for address that are
	// older than lastSeenTxid, newest-first (up to 25 per page).
	//
	// This is the incremental-sync complement to Transactions: the mobile app
	// stores confirmed transactions locally, passes the oldest stored txid here
	// to page back through history without re-fetching what it already has.
	//
	// Maps to Esplora GET /address/{address}/txs/chain/{last_seen_txid}.
	// Only confirmed transactions are returned by this endpoint.
	TransactionsSince(ctx context.Context, address, lastSeenTxid string) ([]Transaction, error)
}
