package bitcoin

// TxStatus is the confirmation state of a transaction.
type TxStatus struct {
	// Confirmed is true when the transaction has at least one confirmation.
	Confirmed bool `json:"confirmed"`

	// BlockHeight is the height of the block that confirmed this transaction.
	// Zero when Confirmed is false.
	BlockHeight uint32 `json:"block_height,omitempty"`

	// BlockTime is the Unix timestamp of the confirming block.
	// Zero when Confirmed is false.
	BlockTime int64 `json:"block_time,omitempty"`
}

// TxInput is one input of a Bitcoin transaction.
//
// It describes a coin being spent: which previous output is being consumed
// and, thanks to Esplora resolving the prevout, which address owned it.
// The mobile app uses Vin to determine direction: if any input address
// belongs to the wallet, the transaction is a send; if only outputs match,
// it is a receive.
type TxInput struct {
	// PrevTxid is the txid of the transaction whose output is being spent.
	PrevTxid string `json:"prev_txid"`

	// PrevVout is the output index within PrevTxid that is being spent.
	PrevVout uint32 `json:"prev_vout"`

	// Address is the address that owned the spent output.
	// Esplora resolves this from prevout.scriptpubkey_address at no extra cost.
	// Empty for coinbase inputs and non-standard scripts.
	Address string `json:"address,omitempty"`
}

// TxOutput is one output of a Bitcoin transaction.
type TxOutput struct {
	// Index is the output's position within the transaction (0-based).
	// Stored here so callers do not have to track array position separately.
	Index uint32 `json:"index"`

	// Value is the output value in satoshis.
	Value uint64 `json:"value"`

	// Address is the bech32(m) address this output pays to.
	// Empty for non-standard scripts (OP_RETURN, bare multisig, etc.).
	Address string `json:"address,omitempty"`
}

// Transaction is a Bitcoin transaction as served by the chain service.
//
// The shape is intentionally lean: it carries what the mobile wallet needs to
// render a transaction list and compute confirmation counts. Raw scripts and
// input details are omitted — the builder on the client side already has them
// at construction time, and re-fetching them per transaction would multiply
// provider calls for no wallet benefit.
type Transaction struct {
	// Txid is the transaction identifier, hex-encoded (64 characters).
	Txid string `json:"txid"`

	// Fee is the miner fee paid, in satoshis.
	// May be zero for coinbase transactions.
	Fee uint64 `json:"fee"`

	// Status is the confirmation state of the transaction.
	Status TxStatus `json:"status"`

	// Vin is the list of inputs — the coins being spent by this transaction.
	// The mobile app compares input addresses against known wallet addresses to
	// determine whether the transaction is a send (wallet address in Vin) or a
	// receive (wallet address only in Vout).
	Vin []TxInput `json:"vin"`

	// Vout is the list of outputs. The mobile app uses this to determine
	// which outputs belong to the wallet address and how much was received
	// or sent (by comparing with known wallet addresses).
	Vout []TxOutput `json:"vout"`
}
