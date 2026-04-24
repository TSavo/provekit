import { readFileSync, existsSync, writeFileSync, mkdirSync, unlinkSync } from "fs";
import { join, dirname } from "path";
import { createHash, randomBytes } from "crypto";
import * as vm from "vm";
import { LLMProvider } from "./llm";

export type HarnessOutcomeKind =
  | "pass"
  | "encoding-gap"
  | "harness-error"
  | "untestable"
  | "timeout"
  | "synthesis-failed";

export interface HarnessOutcome {
  kind: HarnessOutcomeKind;
  message: string;
  harnessCode?: string;
  untestableReason?: string;
}

export interface SynthesisInput {
  functionSource: string;
  claim: string;
  smt2: string;
  contractKey: string;
  functionName: string;
  importedTypes?: string;
  testOracleContext?: string;
}

const PROMPT_CACHE: { template: string | null } = { template: null };

function loadPromptTemplate(projectRoot: string): string {
  if (PROMPT_CACHE.template) return PROMPT_CACHE.template;
  const candidates = [
    join(projectRoot, "prompts", "harness_synthesis.md"),
    join(__dirname, "..", "prompts", "harness_synthesis.md"),
  ];
  for (const p of candidates) {
    if (existsSync(p)) {
      PROMPT_CACHE.template = readFileSync(p, "utf-8");
      return PROMPT_CACHE.template;
    }
  }
  throw new Error("harness_synthesis.md not found");
}

function buildSynthesisPrompt(template: string, input: SynthesisInput): string {
  const task = [
    "",
    "---",
    "",
    "## Your Specific Task",
    "",
    `Contract: \`${input.contractKey}\``,
    "",
    "### Natural-language claim",
    input.claim,
    "",
    "### SMT-LIB block that was proven",
    "```smt2",
    input.smt2,
    "```",
    "",
    "### Function source",
    "```typescript",
    input.functionSource,
    "```",
  ];

  if (input.importedTypes) {
    task.push("");
    task.push("### Imported type definitions");
    task.push("```typescript");
    task.push(input.importedTypes);
    task.push("```");
  }

  if (input.testOracleContext) {
    task.push("");
    task.push(input.testOracleContext);
  }

  task.push("");
  task.push("Emit your harness now, following the format specified above.");

  return template + task.join("\n");
}

function parseSynthesisResponse(text: string): {
  harness: string | null;
  untestable: string | null;
} {
  const untestableMatch = text.match(/\/\/\s*UNTESTABLE:\s*(.+)$/m);
  const codeMatch = text.match(/```(?:javascript|js)?\s*\n([\s\S]*?)```/);

  if (codeMatch && codeMatch[1]) {
    return { harness: codeMatch[1].trim(), untestable: null };
  }
  if (untestableMatch && untestableMatch[1]) {
    return { harness: null, untestable: untestableMatch[1].trim() };
  }
  return { harness: null, untestable: null };
}

export async function synthesizeHarness(
  input: SynthesisInput,
  provider: LLMProvider,
  model: string,
  projectRoot: string
): Promise<{ harness: string | null; untestable: string | null; raw: string }> {
  const template = loadPromptTemplate(projectRoot);
  const prompt = buildSynthesisPrompt(template, input);

  let response;
  try {
    response = await provider.complete(prompt, {
      model,
      systemPrompt: "You synthesize empirical test harnesses for formally-proven claims. Follow the format strictly. Emit either one ```javascript code block OR a single // UNTESTABLE: line. Nothing else.",
    });
  } catch (err: any) {
    return { harness: null, untestable: null, raw: `synthesis-error: ${err?.message?.slice(0, 120) || "unknown"}` };
  }

  const parsed = parseSynthesisResponse(response.text);
  return { ...parsed, raw: response.text.slice(0, 2000) };
}

// vm.createContext here is a containment mechanism, not a security boundary.
// The sandbox holds host-realm function references (functionUnderTest,
// functionUnderTestClass) that a sufficiently motivated adversarial harness
// could walk back into the host realm via prototype-chain escapes. That is
// acceptable in this pipeline because the code being exercised is the user's
// own project source, loaded via the user's own require resolver — the trust
// boundary is "same as running `node dist/cli.js` directly," which already
// runs arbitrary project code. The vm.context exists to (a) give the harness
// a curated set of globals it can reason about, (b) enforce the timeout, and
// (c) prevent accidental global-namespace pollution between contracts — not
// to contain deliberate abuse.
export async function runHarness(
  harnessCode: string,
  fn: (...args: any[]) => any,
  fnClass: any,
  timeoutMs: number = 3000
): Promise<HarnessOutcome> {
  // Deterministic stubs for sources of nondeterminism. Harnesses testing
  // claims about pure logic shouldn't see different outputs across runs;
  // the clock and RNG should be pinned. Harnesses that specifically need
  // to test time or random behaviour can override these in their own
  // code — the stubs just establish a default.
  const stubbedMath = Object.create(Math);
  Object.defineProperty(stubbedMath, "random", { value: () => 0.5, writable: true, configurable: true });
  const stubbedDate = new Proxy(Date, {
    get(target, prop, receiver) {
      if (prop === "now") return () => 0;
      return Reflect.get(target, prop, target);
    },
  });

  const sandbox: any = {
    functionUnderTest: fn,
    functionUnderTestClass: fnClass,
    console: {
      log: () => {},
      warn: () => {},
      error: () => {},
    },
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    Promise,
    Error,
    TypeError,
    RangeError,
    SyntaxError,
    Array,
    Object,
    Number,
    String,
    Boolean,
    JSON,
    Math: stubbedMath,
    Date: stubbedDate,
    Symbol,
    Map,
    Set,
    WeakMap,
    WeakSet,
    Reflect,
    BigInt,
    isNaN,
    isFinite,
    parseInt,
    parseFloat,
    encodeURIComponent,
    decodeURIComponent,
    globalThis: {},
  };
  sandbox.globalThis = sandbox;

  vm.createContext(sandbox);

  const wrapped = `(async () => {\n${harnessCode}\n})()`;

  let promise: Promise<any>;
  try {
    const script = new vm.Script(wrapped, { filename: "<harness>" });
    promise = script.runInContext(sandbox);
  } catch (err: any) {
    return {
      kind: "harness-error",
      message: `harness failed to parse/initialize: ${err?.message?.slice(0, 200) || "unknown"}`,
      harnessCode,
    };
  }

  let timer: NodeJS.Timeout | null = null;
  try {
    await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("__HARNESS_TIMEOUT__")), timeoutMs);
      }),
    ]);
    if (timer) clearTimeout(timer);
    return { kind: "pass", message: "harness completed without throwing", harnessCode };
  } catch (err: any) {
    if (timer) clearTimeout(timer);
    const msg = String(err?.message || err);
    if (msg === "__HARNESS_TIMEOUT__") {
      return { kind: "timeout", message: `harness exceeded ${timeoutMs}ms`, harnessCode };
    }
    if (msg.startsWith("encoding-gap:")) {
      return { kind: "encoding-gap", message: msg, harnessCode };
    }
    if (msg.startsWith("harness-error:")) {
      return { kind: "harness-error", message: msg, harnessCode };
    }
    return {
      kind: "harness-error",
      message: `unclassified throw: ${msg.slice(0, 200)}`,
      harnessCode,
    };
  }
}

export class HarnessCache {
  private cacheDir: string;

  constructor(projectRoot: string) {
    this.cacheDir = join(projectRoot, ".neurallog", "harnesses");
  }

  private cacheKey(smt2: string, functionSource: string, depsSource?: string): string {
    const h = createHash("sha256");
    h.update(smt2);
    h.update("\n---\n");
    h.update(functionSource);
    if (depsSource) {
      h.update("\n---deps---\n");
      h.update(depsSource);
    }
    return h.digest("hex").slice(0, 16);
  }

  get(smt2: string, functionSource: string, depsSource?: string): { harness?: string; untestable?: string; auditValid?: boolean; auditNote?: string } | null {
    const key = this.cacheKey(smt2, functionSource, depsSource);
    const path = join(this.cacheDir, `${key}.json`);
    if (!existsSync(path)) return null;
    try {
      return JSON.parse(readFileSync(path, "utf-8"));
    } catch {
      return null;
    }
  }

  putAudit(
    smt2: string,
    functionSource: string,
    audit: { valid: boolean; note: string },
    depsSource?: string
  ): void {
    const existing = this.get(smt2, functionSource, depsSource) || {};
    const key = this.cacheKey(smt2, functionSource, depsSource);
    mkdirSync(this.cacheDir, { recursive: true });
    const path = join(this.cacheDir, `${key}.json`);
    try {
      writeFileSync(
        path,
        JSON.stringify(
          {
            ...existing,
            auditValid: audit.valid,
            auditNote: audit.note,
            auditedAt: new Date().toISOString(),
          },
          null,
          2
        )
      );
    } catch (e: any) {
      console.log(`[harness-cache] putAudit failed: ${e?.message?.slice(0, 60) || "unknown"}`);
    }
  }

  put(
    smt2: string,
    functionSource: string,
    value: { harness?: string | null; untestable?: string | null },
    depsSource?: string
  ): void {
    const key = this.cacheKey(smt2, functionSource, depsSource);
    mkdirSync(this.cacheDir, { recursive: true });
    const path = join(this.cacheDir, `${key}.json`);
    try {
      writeFileSync(
        path,
        JSON.stringify(
          {
            harness: value.harness || undefined,
            untestable: value.untestable || undefined,
            cachedAt: new Date().toISOString(),
          },
          null,
          2
        )
      );
    } catch (e: any) {
      console.log(`[harness-cache] put failed: ${e?.message?.slice(0, 60) || "unknown"}`);
    }
  }
}

import { instrumentForSnapshot } from "./runtime/snapshotInstrumentation.js";
import { serializeValue } from "./runtime/valueSerializer.js";
import { loadModuleWithPrivates } from "./moduleLoader.js";
import { traces, traceValues } from "./db/schema/index.js";
import type { Db } from "./db/index.js";

// Module-level ref-counted stubs so concurrent runHarnessWithTrace calls
// don't race on Math.random / Date.now. Each call pushes the stubs on entry
// (0→1 installs, >1 is a no-op). The finally block pops them (N→0 restores).
// This keeps every caller's instrumented code seeing deterministic values,
// and leaves the real globals intact outside the active window.
let stubRefCount = 0;
let origRandom: typeof Math.random | null = null;
let origNow: typeof Date.now | null = null;

function pushDeterminismStubs(): void {
  if (stubRefCount === 0) {
    origRandom = Math.random;
    origNow = Date.now;
    Math.random = () => 0.5;
    Date.now = () => 0;
  }
  stubRefCount++;
}

function popDeterminismStubs(): void {
  stubRefCount--;
  if (stubRefCount === 0 && origRandom && origNow) {
    Math.random = origRandom;
    Date.now = origNow;
    origRandom = null;
    origNow = null;
  }
}

export interface RunHarnessWithTraceArgs {
  db: Db;
  clauseId: number;
  sourcePath: string;
  functionName: string;
  signalLine: number;
  captureNames: string[];
  inputs: Record<string, unknown>;
  timeoutMs?: number;
}

export interface RunHarnessWithTraceResult {
  outcomeKind: "returned" | "threw" | "untestable";
  returnValue?: unknown;
  error?: string;
  traceId: number;
}

export async function runHarnessWithTrace(args: RunHarnessWithTraceArgs): Promise<RunHarnessWithTraceResult> {
  const {
    db,
    clauseId,
    sourcePath,
    functionName,
    signalLine,
    captureNames,
    inputs,
    timeoutMs = 3000,
  } = args;

  // Unique suffix per call. Two runs sharing a globalThis name clobber each
  // other's snapshot arrays — one run's instrumented code calls the other
  // run's function and writes into the wrong array, leaving the first run's
  // trace empty. Same reasoning applies to the on-disk instrumented file:
  // two concurrent runs against the same source would race on the path.
  const callId = randomBytes(8).toString("hex");
  const snapshotFnName = `__neurallog_snapshot_${callId}__`;
  const instrumentedPath = sourcePath.replace(/\.ts$/, `.__instrumented_${callId}__.ts`);

  const originalSource = readFileSync(sourcePath, "utf-8");
  const instrumented = instrumentForSnapshot(originalSource, {
    signalLine,
    captureNames,
    snapshotFnName,
  });

  writeFileSync(instrumentedPath, instrumented);

  const capturedSnapshots: { fnName: string; line: number; locals: Record<string, unknown> }[] = [];

  (globalThis as any)[snapshotFnName] = (fnName: string, line: number, locals: Record<string, unknown>) => {
    capturedSnapshots.push({ fnName, line, locals: { ...locals } });
  };

  // Stub Math.random / Date.now under a ref count so concurrent runs don't
  // race: first active run installs the stubs, last run restores. Each run
  // still gets deterministic values for its instrumented execution.
  pushDeterminismStubs();

  const inputsHash = createHash("sha256")
    .update(JSON.stringify(inputs))
    .digest("hex")
    .slice(0, 16);

  let outcome: RunHarnessWithTraceResult = {
    outcomeKind: "threw",
    error: "not executed",
    traceId: -1,
  };

  try {
    const mod = loadModuleWithPrivates(instrumentedPath);
    const fn = mod[functionName];
    if (typeof fn !== "function") {
      outcome = {
        outcomeKind: "untestable",
        error: `export ${functionName} not a function`,
        traceId: -1,
      };
    } else {
      const callArgs = Object.values(inputs);
      let result: unknown;
      try {
        result = await Promise.race([
          (async () => fn(...callArgs))(),
          new Promise((_, reject) => setTimeout(() => reject(new Error("__TIMEOUT__")), timeoutMs)),
        ]);
        outcome = {
          outcomeKind: "returned",
          returnValue: result,
          traceId: -1,
        };
      } catch (err: any) {
        outcome = {
          outcomeKind: "threw",
          error: String(err?.message || err).slice(0, 500),
          traceId: -1,
        };
      }
    }
  } finally {
    popDeterminismStubs();
    delete (globalThis as any)[snapshotFnName];
    try {
      unlinkSync(instrumentedPath);
    } catch {
      // ignore
    }
  }

  // Persist — runs AFTER Date.now stub is restored, so capturedAt uses real time.
  const outcomeValueId =
    outcome.outcomeKind === "returned"
      ? serializeValue(db, outcome.returnValue)
      : outcome.outcomeKind === "threw"
        ? serializeValue(db, outcome.error)
        : null;

  const traceRow = db.insert(traces).values({
    clauseId,
    capturedAt: Date.now(),
    outcomeKind: outcome.outcomeKind,
    outcomeValueId: outcomeValueId ?? undefined,
    untestableReason: outcome.outcomeKind === "untestable" ? outcome.error : undefined,
    inputsHash,
  }).returning().get();

  for (const snap of capturedSnapshots) {
    for (const [name, value] of Object.entries(snap.locals)) {
      const valueId = serializeValue(db, value);
      db.insert(traceValues).values({
        traceId: traceRow.id,
        nodeId: `${sourcePath}:${snap.line}:${name}`,
        iterationIndex: null,
        rootValueId: valueId,
      }).run();
    }
  }

  outcome.traceId = traceRow.id;
  return outcome;
}
