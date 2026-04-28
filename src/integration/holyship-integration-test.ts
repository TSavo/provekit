/**
 * Holyship integration smoke test (task #131, Channel 2 surface verification).
 *
 * This file is NOT a runtime test; it is a compile-time assertion that the
 * library surface declared in src/index.ts is callable from outside the fix
 * loop's own internals. It exercises:
 *
 *   - importing every entry point named in the spec's "Distribution surface"
 *   - constructing a minimal BugSignal + BugLocus + RemediationPlan
 *   - calling runFixLoop with a stubbed LLMProvider
 *   - asserting the return value is FixLoopResult-shaped
 *
 * It does not invoke a real LLM, does not open a real database, and does not
 * touch the filesystem. The runFixLoop call sits behind `if (false)` so the
 * test asserts ONLY that the imports resolve and types line up. Wiring an
 * actual stub-driven runFixLoop run is a v2 concern — runFixLoop currently
 * requires a real Db handle (better-sqlite3) and an InvestigateReport
 * upstream, neither of which is reasonably stubbable in a smoke test.
 *
 * If this file fails to type-check, the public surface is broken and any
 * IDE / agent runtime / platform integrating ProvekIt will fail at the
 * import boundary.
 *
 * Imported via the relative path because we don't need the package name to
 * resolve in dev. In prod, integrators import from "provekit" which resolves
 * via the "exports" map in package.json.
 */

import {
  // Pipeline
  runFixLoop,
  // Standing-runtime gate
  verifyAll,
  verifyAllCached,
  // Retrospective intake
  extractIntent,
  // Constraint store
  readInvariants,
  writeInvariant,
  retireInvariant,
  buildStoredInvariant,
  hashInvariant,
  // LLM provider
  StubLLMProvider,
} from "../index.js";

import type {
  RunFixLoopArgs,
  BugSignal,
  BugLocus,
  RemediationPlan,
  FixLoopResult,
  FixBundle,
  AuditEntry,
  LLMProvider,
  StoredInvariant,
  InvariantClaim,
  VerifyReport,
  VerifyOptions,
  InvariantVerdict,
  CachedVerifyReport,
  IntentReport,
  RetrospectiveIntakeInput,
  Path,
  PathStep,
  PathVerdict,
} from "../index.js";

// ---------------------------------------------------------------------------
// Compile-time assertions: every entry point is importable as the right kind.
// ---------------------------------------------------------------------------

// Functions are functions.
const _runFixLoop: typeof runFixLoop = runFixLoop;
const _verifyAll: typeof verifyAll = verifyAll;
const _verifyAllCached: typeof verifyAllCached = verifyAllCached;
const _extractIntent: typeof extractIntent = extractIntent;
const _readInvariants: typeof readInvariants = readInvariants;
const _writeInvariant: typeof writeInvariant = writeInvariant;
const _retireInvariant: typeof retireInvariant = retireInvariant;
const _buildStoredInvariant: typeof buildStoredInvariant = buildStoredInvariant;
const _hashInvariant: typeof hashInvariant = hashInvariant;

// Reference everything so unused-import warnings can't mask a missing export.
void _runFixLoop;
void _verifyAll;
void _verifyAllCached;
void _extractIntent;
void _readInvariants;
void _writeInvariant;
void _retireInvariant;
void _buildStoredInvariant;
void _hashInvariant;

// ---------------------------------------------------------------------------
// Minimal Holyship-shaped fixture.
//
// Holyship's gate library would, on `report` from an agent, build something
// like the structures below and hand them to runFixLoop.
// ---------------------------------------------------------------------------

const minimalSignal: BugSignal = {
  source: "smoke",
  rawText: "smoke test bug report",
  summary: "smoke",
  failureDescription: "smoke",
  codeReferences: [{ file: "src/smoke.ts", line: 1 }],
};

const minimalLocus: BugLocus = {
  file: "src/smoke.ts",
  line: 1,
  confidence: 0.9,
  primaryNode: "smoke_node",
  containingFunction: "smoke_node",
  relatedFunctions: [],
  dataFlowAncestors: [],
  dataFlowDescendants: [],
  dominanceRegion: [],
  postDominanceRegion: [],
};

const minimalPlan: RemediationPlan = {
  signal: minimalSignal,
  locus: minimalLocus,
  primaryLayer: "code_patch",
  secondaryLayers: [],
  artifacts: [],
  rationale: "smoke test plan",
};

// Stubbed LLM — never called because we don't actually run the loop.
const stubLLM: LLMProvider = new StubLLMProvider(new Map());

// ---------------------------------------------------------------------------
// runFixLoop call site (compile-time only — guarded by `if (false)`).
// The point is to prove the args type accepts a stub LLM.
// ---------------------------------------------------------------------------

export async function holyshipSmoke(): Promise<FixLoopResult | null> {
  // `if (false)` — do not execute. Existing for type-check only.
  // runFixLoop requires a real Db handle (better-sqlite3) which we don't
  // construct here; v2 will add a memory-backed Db stub for true E2E testing
  // of the public surface.
  if (Math.random() < 0) {
    const args: RunFixLoopArgs = {
      signal: minimalSignal,
      locus: minimalLocus,
      plan: minimalPlan,
      // @ts-expect-error — Db is not stubbable today; this is a compile fence.
      db: null,
      llm: stubLLM,
      options: {
        autoApply: false,
        maxComplementarySites: 10,
        confidenceThreshold: 0.8,
      },
    };
    const result: FixLoopResult = await runFixLoop(args);

    // Shape assertions: FixLoopResult has `bundle`, `outcome`, `auditTrail`.
    const _bundle: FixBundle | null = result.bundle;
    const _audit: AuditEntry[] = result.auditTrail;
    void _bundle;
    void _audit;
    return result;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Type-only sanity references for the type surface. If any of these fail to
// resolve, the surface is missing a name an integrator would need.
// ---------------------------------------------------------------------------

type _SurfaceTypeRefs = {
  storedInvariant: StoredInvariant;
  invariantClaim: InvariantClaim;
  verifyReport: VerifyReport;
  verifyOptions: VerifyOptions;
  invariantVerdict: InvariantVerdict;
  cachedVerifyReport: CachedVerifyReport;
  intentReport: IntentReport;
  retrospectiveIntakeInput: RetrospectiveIntakeInput;
  path: Path;
  pathStep: PathStep;
  pathVerdict: PathVerdict;
};

// Force the type alias to be referenced so it's not tree-shaken from checking.
export type __HolyshipSurfaceTypeRefs = _SurfaceTypeRefs;
