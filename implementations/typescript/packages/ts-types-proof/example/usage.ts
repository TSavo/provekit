/**
 * Example: a user who installed @provekit/ts-types-proof for the
 * types gets the protocol substrate along for the ride.
 *
 * Run with: npx tsx kits/ts-types-proof/example/usage.ts
 */

// One import. The user wanted typed helpers; the import also
// auto-registers 13 V8 bridge declarations in the protocol's
// registry as a side effect.
import { parseInt, abs, num, str, listBridges } from "../src/index.js";

// Build an IR term using the bridged primitives. Looks like normal
// TypeScript; under the hood, each call emits an IR node.
const term1 = parseInt(str("42"));
const term2 = abs(num(-7));

console.log("=== terms emitted by the kit's primitives ===");
console.log(JSON.stringify(term1, null, 2));
console.log(JSON.stringify(term2, null, 2));

console.log();
console.log("=== bridges registered as a side effect ===");
const bridges = listBridges();
console.log(`${bridges.length} V8 bridge declarations register at module load:`);
for (const b of bridges) {
  console.log(`  ${b.irName.padEnd(28)} -> ${b.targetLayer}`);
}

console.log();
console.log("Anyone who imports this package gets the protocol substrate");
console.log("for free. They opted in to typed helpers; the protocol came");
console.log("along. Adoption asymmetry: refusing means giving up the");
console.log("helpers, which nobody does.");
