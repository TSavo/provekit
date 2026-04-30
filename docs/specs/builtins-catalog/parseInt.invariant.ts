/**
 * Built-in contract: parseInt — BRIDGE FORM
 *
 * Spec source: ECMA-262 §7.1.4.1 (parseInt)
 * Kit: provekit-ts@1.0
 * Status: SEED MEMENTO of the global proof DAG (bridge form).
 *
 * --------------------------------------------------------------------------
 * Why this file is short
 * --------------------------------------------------------------------------
 *
 * The TS-kit's job for parseInt is NOT to redefine parseInt's contract.
 * parseInt's behavior is specified by ECMA-262 §7.1.4.1, implemented by
 * V8 (and SpiderMonkey, JavaScriptCore, etc.), grounded in IEEE 754 for
 * numeric edge cases, ultimately rooted in hardware FPU verification.
 *
 * Each of those layers publishes its own contract once. The TS-kit
 * BRIDGES from the TypeScript surface symbol `global.parseInt` to the
 * deeper-layer contract. The bridge composes by hash; it does NOT
 * redefine.
 *
 * This is exactly the architectural pattern described in:
 *   docs/specs/2026-04-29-correctness-is-a-hash.md §"Adding propositions"
 *   docs/specs/2026-04-29-correctness-is-a-hash.md §"What ProvekIt is"
 *
 * --------------------------------------------------------------------------
 * The previous form of this file (~120 lines redefining 17 properties)
 * was REDUNDANT WORK. It restated what V8 / ECMA-262 / IEEE 754 already
 * specify. The bridge form points at those specifications by hash.
 *
 * Run `npx tsx scripts/cross-language-demo/bridges/layered-bridges-demo.ts`
 * for an operational demo of the layered chain.
 * --------------------------------------------------------------------------
 */

import { property, bridge } from 'provekit/ir';

property("parseIntBridgesV8",
  bridge(
    "global.parseInt",          // TS surface symbol
    "v8::Number::parseInt@12.4" // V8's published contract identity
  )
);

// Cross-engine: the same TS-kit's parseInt also bridges to other
// JavaScript engines that ship parseInt contracts. SpiderMonkey,
// JavaScriptCore, ChakraCore — each may publish their own parseInt
// contract grounded in ECMA-262. Consumers running on Bun (uses
// JavaScriptCore) inherit a different mid-chain than Node (uses V8),
// but both ground at the same ECMA-262 spec leaf.
//
// property("parseIntBridgesJSC",
//   bridge("global.parseInt", "jsc::Number::parseInt@latest"));
//
// property("parseIntBridgesSpiderMonkey",
//   bridge("global.parseInt", "sm::Number::parseInt@latest"));

// What's left for the TS-kit to attest directly: nothing structural.
// Behavioral edge cases ABOVE the JS engine level (e.g., behavior in
// a specific TS-typed surface like `parseInt(unknown)` requiring a
// type narrowing) might warrant TS-kit-specific properties. For
// purely runtime behavior, the bridges are sufficient.
