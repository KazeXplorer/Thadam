/**
 * Prints this build's environment catalogue as canonical JSON.
 *
 * Must produce byte-identical output to `core`'s `dump-environments`
 * binary. `registry/conformance/check.sh` diffs both against
 * `registry/conformance/environments.json`.
 *
 * Written by hand rather than with `JSON.stringify` so that the field order
 * and formatting are stated explicitly on both sides, instead of depending on
 * two languages' serialisers happening to agree.
 */

import { ENVIRONMENTS, chainOf, environmentOf } from "../src/index.ts";

const sorted = [...ENVIRONMENTS].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));

const lines: string[] = ["{", '  "environments": ['];

sorted.forEach((environment, index) => {
  const separator = index + 1 === sorted.length ? "" : ",";
  lines.push(
    "    {",
    `      "id": "${environment.id}",`,
    `      "chain": "${chainOf(environment.id)}",`,
    `      "environment": "${environmentOf(environment.id)}",`,
    `      "displayName": "${environment.displayName}",`,
    `      "kind": "${environment.kind}"`,
    `    }${separator}`,
  );
});

lines.push("  ]", "}");

console.log(lines.join("\n"));
