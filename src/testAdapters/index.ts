/**
 * Test-adapter registry. Framework detection happens in ../testOracle.ts;
 * this module maps a detected framework identifier to the adapter that
 * implements it. Adapters import Adapter interface types from ./Adapter.
 */

import type { TestAdapter } from "./Adapter";
import { VitestAdapter } from "./vitest";
import { JestAdapter } from "./jest";
import { MochaAdapter } from "./mocha";
import { NodeTestAdapter } from "./nodeTest";

export type { TestAdapter, TestInvocation, TestOutcome, TestOutcomeKind } from "./Adapter";

const ADAPTERS: Record<string, TestAdapter> = {};

function register(adapter: TestAdapter): void {
  ADAPTERS[adapter.framework] = adapter;
}

register(new VitestAdapter());
register(new JestAdapter());
register(new MochaAdapter());
register(new NodeTestAdapter());

export function getAdapter(framework: string): TestAdapter | null {
  return ADAPTERS[framework] || null;
}

export function listAdapters(): TestAdapter[] {
  return Object.values(ADAPTERS);
}
