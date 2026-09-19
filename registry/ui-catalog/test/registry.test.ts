import assert from "node:assert/strict";
import { test } from "node:test";

import {
  BITCOIN_NETWORKS,
  CHAIN_IDS,
  ENVIRONMENTS,
  chainOf,
  environmentOf,
  findEnvironment,
  parseEnvironmentId,
} from "../src/index.ts";

test("every network appears in the catalogue exactly once", () => {
  for (const network of BITCOIN_NETWORKS) {
    const matches = ENVIRONMENTS.filter(
      (environment) => environment.id === `bitcoin:${network}`,
    );
    assert.equal(matches.length, 1, `bitcoin:${network} appears ${matches.length} times`);
  }
  assert.equal(ENVIRONMENTS.length, BITCOIN_NETWORKS.length);
});

test("catalogue ids and display names are unique", () => {
  assert.equal(new Set(ENVIRONMENTS.map((e) => e.id)).size, ENVIRONMENTS.length);
  assert.equal(new Set(ENVIRONMENTS.map((e) => e.displayName)).size, ENVIRONMENTS.length);
});

test("findEnvironment resolves every catalogued id", () => {
  for (const environment of ENVIRONMENTS) {
    assert.deepEqual(findEnvironment(environment.id), environment);
  }
});

test("identifiers round-trip through parse", () => {
  for (const environment of ENVIRONMENTS) {
    const result = parseEnvironmentId(environment.id);
    assert.ok(result.ok, `${environment.id} did not parse`);
    assert.equal(result.id, environment.id);
    assert.equal(chainOf(result.id), "bitcoin");
    assert.equal(environmentOf(result.id), environment.id.split(":")[1]);
  }
});

test("malformed identifiers are rejected", () => {
  for (const input of ["", "bitcoin", "testnet4", ":", "bitcoin:testnet4:extra"]) {
    const result = parseEnvironmentId(input);
    assert.ok(!result.ok, `${JSON.stringify(input)} was not rejected`);
  }
});

test("unknown chains and environments report which half failed", () => {
  const chain = parseEnvironmentId("ethereum:sepolia");
  assert.ok(!chain.ok);
  assert.deepEqual(chain.error, { kind: "unknown-chain", chain: "ethereum" });

  const environment = parseEnvironmentId("bitcoin:testnet3");
  assert.ok(!environment.ok);
  assert.deepEqual(environment.error, {
    kind: "unknown-environment",
    chain: "bitcoin",
    environment: "testnet3",
  });
});

// ARCHITECTURE.md §2 — the `ui-catalog` half of the guarantee. The type system
// already rejects `bitcoin:mainnet` at compile time; this asserts the runtime
// parser agrees, since identifiers arrive from persisted state and URLs where
// the compiler cannot see them.
test("mainnet does not parse as an environment id", () => {
  for (const input of [
    "bitcoin:mainnet",
    "bitcoin:main",
    "bitcoin:bitcoin",
    "bitcoin:livenet",
    "BITCOIN:MAINNET",
  ]) {
    assert.ok(!parseEnvironmentId(input).ok, `${input} parsed into an environment id`);
  }
});

test("no catalogued environment is named like mainnet", () => {
  for (const environment of ENVIRONMENTS) {
    assert.ok(
      !["mainnet", "main", "livenet"].includes(environmentOf(environment.id)),
      `catalogue contains ${environment.id}`,
    );
  }
  assert.ok(!(BITCOIN_NETWORKS as readonly string[]).includes("mainnet"));
  assert.deepEqual([...CHAIN_IDS], ["bitcoin"]);
});
