/**
 * Chain and environment identifiers.
 *
 * This is `ui-catalog`, the display half of the registry. It holds *identity*
 * only — which environments exist and what to call them.
 *
 * It deliberately does not hold consensus constants (bech32 HRPs, version
 * bytes, coin types). Only the signer needs those, the signer is WASM, and
 * duplicating them here would create a second copy to keep correct for no
 * consumer. See ARCHITECTURE.md §6 and `registry/README.md`.
 */

/** Every chain this build knows about. */
export const CHAIN_IDS = ["bitcoin"] as const;

/** A chain this build knows about. */
export type ChainId = (typeof CHAIN_IDS)[number];

/**
 * Every Bitcoin network this build can reach.
 *
 * There is no `"mainnet"` entry, and there must never be one. ARCHITECTURE.md
 * §2: mainnet is unrepresentable, not disabled. Because `EnvironmentId` below
 * is derived from this array, adding an entry here is the only way to make
 * `"bitcoin:mainnet"` a valid value anywhere in the client — which is exactly
 * why the rule is enforceable.
 *
 * testnet3 is absent on purpose: its difficulty reset rule makes it unusable
 * for timing-sensitive work, and its addresses are indistinguishable from
 * testnet4's.
 */
export const BITCOIN_NETWORKS = ["testnet4", "signet", "regtest"] as const;

/** A Bitcoin network this build can reach. */
export type BitcoinNetwork = (typeof BITCOIN_NETWORKS)[number];

/**
 * A chain together with one of its environments, as `chain:environment`.
 *
 * The type is a template literal over {@link BITCOIN_NETWORKS}, so
 * `"bitcoin:mainnet"` is a type error rather than a runtime check that could
 * be skipped.
 */
export type EnvironmentId = `bitcoin:${BitcoinNetwork}`;

/** Why a string was not a valid {@link EnvironmentId}. */
export type ParseEnvironmentIdError =
  | { readonly kind: "malformed" }
  | { readonly kind: "unknown-chain"; readonly chain: string }
  | {
      readonly kind: "unknown-environment";
      readonly chain: ChainId;
      readonly environment: string;
    };

/** The outcome of {@link parseEnvironmentId}. */
export type ParseResult =
  | { readonly ok: true; readonly id: EnvironmentId }
  | { readonly ok: false; readonly error: ParseEnvironmentIdError };

/** Renders a {@link ParseEnvironmentIdError} for a human. */
export function describeParseError(error: ParseEnvironmentIdError): string {
  switch (error.kind) {
    case "malformed":
      return "expected exactly one 'chain:environment' pair";
    case "unknown-chain":
      return `unknown chain '${error.chain}'`;
    case "unknown-environment":
      return `unknown ${error.chain} environment '${error.environment}'`;
  }
}

function isChainId(value: string): value is ChainId {
  return (CHAIN_IDS as readonly string[]).includes(value);
}

function isBitcoinNetwork(value: string): value is BitcoinNetwork {
  return (BITCOIN_NETWORKS as readonly string[]).includes(value);
}

/**
 * Parses a `chain:environment` string.
 *
 * Returns a result rather than throwing, because a bad identifier arriving
 * from persisted state or a URL is an expected condition the UI has to
 * render, not an exception.
 */
export function parseEnvironmentId(input: string): ParseResult {
  const parts = input.split(":");
  if (parts.length !== 2) {
    return { ok: false, error: { kind: "malformed" } };
  }

  const [chain, environment] = parts as [string, string];

  if (!isChainId(chain)) {
    return { ok: false, error: { kind: "unknown-chain", chain } };
  }

  if (!isBitcoinNetwork(environment)) {
    return {
      ok: false,
      error: { kind: "unknown-environment", chain, environment },
    };
  }

  return { ok: true, id: `${chain}:${environment}` };
}

/** The chain half of an identifier. */
export function chainOf(id: EnvironmentId): ChainId {
  return id.split(":")[0] as ChainId;
}

/** The environment half of an identifier. */
export function environmentOf(id: EnvironmentId): BitcoinNetwork {
  return id.split(":")[1] as BitcoinNetwork;
}
