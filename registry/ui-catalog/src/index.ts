/**
 * Thadam chain registry — TypeScript.
 *
 * Lookup only. It answers "which environments exist" and "what do we call
 * them". It does not build transactions, derive keys, encode addresses, or
 * reach a network. See ARCHITECTURE.md §6.
 *
 * Consensus constants live in `core` and only there. See
 * `registry/README.md` for why.
 */

export {
  BITCOIN_NETWORKS,
  CHAIN_IDS,
  chainOf,
  describeParseError,
  environmentOf,
  parseEnvironmentId,
} from "./id.ts";

export type {
  BitcoinNetwork,
  ChainId,
  EnvironmentId,
  ParseEnvironmentIdError,
  ParseResult,
} from "./id.ts";

export { ENVIRONMENTS, findEnvironment } from "./environments.ts";

export type { Environment, NetworkKind } from "./environments.ts";
