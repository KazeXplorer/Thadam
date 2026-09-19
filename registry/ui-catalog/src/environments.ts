/** The catalogue. */

import type { EnvironmentId } from "./id.ts";

/** Whether an environment is a shared network or one running on this machine. */
export type NetworkKind = "live" | "local";

/** One entry in the catalogue. */
export interface Environment {
  /** This environment's identifier. */
  readonly id: EnvironmentId;
  /**
   * A name for a human.
   *
   * Displayed next to every address. ARCHITECTURE.md §8: testnet4 and signet
   * addresses are identical, so the network has to be shown, not inferred.
   */
  readonly displayName: string;
  /** Whether this is a shared or a local network. */
  readonly kind: NetworkKind;
}

/**
 * Every environment this build knows about.
 *
 * This list is maintained by hand and duplicates `core`'s
 * `ENVIRONMENTS`. That duplication is deliberate and cheap — it is three
 * entries of identity data with nothing cryptographic in it — and
 * `registry/conformance/check.sh` fails if the two drift apart. Codegen buys
 * nothing at this size; revisit it past roughly ten environments.
 */
export const ENVIRONMENTS: readonly Environment[] = [
  {
    id: "bitcoin:testnet4",
    displayName: "Bitcoin testnet4",
    kind: "live",
  },
  {
    id: "bitcoin:signet",
    displayName: "Bitcoin signet",
    kind: "live",
  },
  {
    id: "bitcoin:regtest",
    displayName: "Bitcoin regtest",
    kind: "local",
  },
] as const;

/** Looks up an environment by its identifier. */
export function findEnvironment(id: EnvironmentId): Environment | undefined {
  return ENVIRONMENTS.find((environment) => environment.id === id);
}
