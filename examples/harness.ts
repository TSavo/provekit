/**
 * Runtime mode harness test.
 *
 * Prerequisites:
 *   1. Run `neurallog analyze examples/inventory.ts` first to generate contracts
 *   2. Then run this: `npx ts-node examples/harness.ts`
 *
 * What it does:
 *   - Creates a pino logger with the neurallog transport
 *   - Simulates inventory operations with real values
 *   - The transport intercepts each log, matches it to a contract,
 *     evaluates the contract against live values with Z3,
 *     and emits proof entries (pass/fail)
 */

import pino from "pino";
import { createNeurallogTransport } from "../src/transport";

// Create logger with neurallog transport
const transport = createNeurallogTransport({
  projectRoot: __dirname + "/..",
});

const logger = pino({ level: "debug" }, transport);

// Simulated DB with real values
let inventory: Record<string, { available: number; reserved: number }> = {
  "SKU-001": { available: 50, reserved: 10 },
  "SKU-002": { available: 0, reserved: 5 },   // edge case: zero available
  "SKU-003": { available: 3, reserved: 20 },
};

// --- Test 1: Normal operation ---
console.log("\n=== Test 1: Normal reservation ===");
const sku1 = inventory["SKU-001"]!;
logger.info({
  _nl: { file: __dirname + "/inventory.ts", line: 18 },
  quantity: 5,
  productId: "SKU-001",
  available: sku1.available,
  reserved: sku1.reserved,
}, "Reserving 5 of SKU-001");

// --- Test 2: Overdraw — quantity > available ---
console.log("\n=== Test 2: Overdraw (quantity > available) ===");
const sku2 = inventory["SKU-002"]!;
logger.info({
  _nl: { file: __dirname + "/inventory.ts", line: 18 },
  quantity: 10,
  productId: "SKU-002",
  available: sku2.available,  // 0
  reserved: sku2.reserved,
}, "Reserving 10 of SKU-002 (should trigger violation)");

// --- Test 3: Negative quantity ---
console.log("\n=== Test 3: Negative quantity ===");
const sku3 = inventory["SKU-003"]!;
logger.info({
  _nl: { file: __dirname + "/inventory.ts", line: 18 },
  quantity: -5,
  productId: "SKU-003",
  available: sku3.available,
  reserved: sku3.reserved,
}, "Reserving -5 of SKU-003 (should trigger violation)");

// --- Test 4: Zero quantity ---
console.log("\n=== Test 4: Zero quantity (degenerate) ===");
logger.info({
  _nl: { file: __dirname + "/inventory.ts", line: 18 },
  quantity: 0,
  productId: "SKU-001",
  available: sku1.available,
  reserved: sku1.reserved,
}, "Reserving 0 of SKU-001 (degenerate no-op)");

// Give async Z3 evaluations time to complete
setTimeout(() => {
  console.log("\n=== Harness complete ===");
  process.exit(0);
}, 5000);
