/**
 * Invariant store — step 1 of the standing-invariant-runtime spec.
 *
 * Per protocol/specs/2026-04-27-standing-invariant-runtime.md: the fix loop's
 * derived constraint must live as a source-controlled, content-addressable
 * artifact at `.provekit/invariants/<sha>.json`. Two runs producing the
 * same constraint write the same file (idempotent). The patch commit and
 * the constraint file travel together; the standing runtime later
 * re-resolves bindings against the AST and Z3-checks every dataflow path
 * to the protected sink.
 *
 * v1 stays minimal: schema + write + read + content-addressable id.
 * No path enumeration, no Z3 checking, no decay detection — those are
 * later steps in the spec's implementation order. This module exists so
 * downstream stages can land independently.
 */

import { mkdirSync, readdirSync, readFileSync, writeFileSync, existsSync } from "fs";
import { join } from "path";
import { createHash } from "crypto";
import type { InvariantClaim, BugLocus, BugSignal, TestArtifact } from "../types.js";

// ---------------------------------------------------------------------------
// Binding tagged union
// ---------------------------------------------------------------------------

/**
 * Local binding: the SMT constant is bound to a specific source span. Drift
 * is detected by re-hashing the span's bytes against the recorded nodeHash.
 */
export interface LocalBinding {
  type: "local";
  smt_constant: string;
  source_expr: string;
  /** Z3 sort. Common values: Int, Bool, Real. */
  sort: string;
  node: {
    filePath: string;
    /** sha256 prefix (16 hex chars) of the byte range at write time. */
    nodeHash: string;
    startLine: number;
    endLine: number;
  };
}

/**
 * Graph binding: the SMT constant is bound to a graph reachability claim.
 * The `relation` names a substrate-side traversal (e.g.
 * "imports_transitively"); `predicate` names a check applied to each
 * reached node (e.g. "no_match"). `predicateArg` is the relation- and
 * predicate-specific argument — for "no_match" against the import-graph
 * relation, it's a glob pattern; future relations may need different
 * arg shapes (the field stays open).
 *
 * Drift detection walks `relation` from `root` and applies `predicate`.
 * If the predicate still holds → resolved. Otherwise → decayed with a
 * graph-shaped reason. No content-hash; the relation walk IS the check.
 */
export interface GraphBinding {
  type: "graph";
  smt_constant: string;
  /**
   * Substrate relation name. v1 supports: "imports_transitively".
   * Future: "data_flow_reaches", "type_subsumed_by", etc.
   */
  relation: "imports_transitively";
  /** The graph traversal's starting node — typically a file path. */
  root: { filePath: string };
  /**
   * Predicate applied to reached nodes. v1 supports:
   * - "no_match": none of the reached nodes' filePaths glob-match
   *   `predicateArg` (the standard "FOO must not transitively reach BAR"
   *   shape).
   */
  predicate: "no_match";
  predicateArg: string;
}

export type Binding = LocalBinding | GraphBinding;

/**
 * Read-time normalizer: legacy persisted invariants stored bindings as
 * the local-only shape without a `type` field. This coerces such bindings
 * to `{ type: "local", ...rest }` so the rest of the pipeline sees a
 * uniform tagged union. Idempotent — already-typed bindings pass through.
 */
export function normalizeBinding(raw: unknown): Binding {
  if (typeof raw !== "object" || raw === null) {
    throw new Error(`normalizeBinding: expected object, got ${typeof raw}`);
  }
  const obj = raw as Record<string, unknown>;
  if (obj.type === "local" || obj.type === "graph") {
    return obj as unknown as Binding;
  }
  // Legacy: presume local. Defensive on field presence.
  if (obj.node && typeof obj.node === "object") {
    return { type: "local", ...obj } as unknown as LocalBinding;
  }
  throw new Error(
    `normalizeBinding: unrecognized binding shape (no type, no node): ${JSON.stringify(raw)}`,
  );
}

// ---------------------------------------------------------------------------
// Stored invariant schema
// ---------------------------------------------------------------------------

/**
 * The on-disk shape. Field set is the contract between the fix loop
 * (writer) and the standing runtime (reader). Schema follows the runtime
 * spec exactly — `id` is the content-addressable filename root.
 *
 * The bindings carry a `node` block with the file path + content-hash of
 * the source span at write time. The standing runtime treats a binding
 * as "decayed" when the file's current content no longer hashes to the
 * recorded value at the recorded byte range.
 */
export interface StoredInvariant {
  /** sha256 prefix (16 hex chars) of (smt assertion + bindings). Filename root. */
  id: string;
  /** ISO-8601 timestamp of the originating fix loop run. */
  createdAt: string;
  /** Free-text user signal that motivated this invariant (BugSignal.summary). */
  originatingBug: string;
  /** The invariant's formal property in SMT form. */
  smt: {
    kind: "arithmetic" | "set_uniqueness" | "cardinality" | "order" | "taint" | "other";
    declarations: string[];
    assertion: string;
  };
  /**
   * Each binding maps an SMT constant to evidence in the codebase. Two
   * binding kinds are supported, distinguished by the `type` discriminant:
   *
   * - "local": binds to a specific source span. Drift detection is a
   *   content-hash compare on the bound bytes. Cheap, the workhorse for
   *   per-callsite invariants (division-by-zero, off-by-one, etc.).
   *
   * - "graph": binds to a graph reachability claim — e.g. "no module
   *   transitively imported from FILE matches PATTERN". Drift detection
   *   walks the relation and re-evaluates the predicate. Required for
   *   architectural axioms like "no LLM in verification path" that don't
   *   reduce to a single source span.
   *
   * Backward compatibility: legacy persisted invariants without a `type`
   * field are read as `type: "local"`. New writes always include the
   * discriminant.
   */
  bindings: Array<Binding>;
  /**
   * Where the patch landed. Path enumeration runs from this site by default.
   *
   * Self-healing binding: `startLine` is the canonical pointer; `functionHash`
   * + `functionOffset` carry recovery information when the file shifts. The
   * resolver's four-way state machine:
   *
   *   1. Line resolves      → holds (direct hit)
   *   2. Line missed but functionHash + offset recover → holds, line self-heals
   *   3. functionHash present but no longer in substrate (content changed) → decayed
   *   4. functionHash present but the function's gone entirely → gone (retire candidate)
   *
   * `function` (name) and the hash/offset pair are optional for backward
   * compatibility with invariants minted before the recovery shape landed.
   */
  callsite: {
    filePath: string;
    function: string | null;
    /** Substrate `subtreeHash` of the containing function at write time. */
    functionHash?: string | null;
    /** `startLine - containingFunction.startLine` at write time. */
    functionOffset?: number | null;
    startLine: number;
    endLine: number;
  };
  /**
   * "callsite" (default): bound nodes ARE the function under test; path
   * enumeration walks backward from the callsite.
   * "sink": bound nodes are the data destination; --adversarial mode
   * walks backward across the entire dataflow graph.
   */
  scope: "callsite" | "sink";
  /** Reference to the regression test that locks this invariant in. */
  regressionTest: {
    filePath: string;
    testName: string;
  } | null;
  /** Git commit sha of the patch that established this invariant. May be null in dry-run. */
  patchSha: string | null;
  /**
   * Tombstone for explicit retirement. When non-null, `provekit verify`
   * skips this invariant. Audit trail preserved.
   */
  retired: {
    at: string;
    reason: string;
  } | null;
}

// ---------------------------------------------------------------------------
// Content addressing
// ---------------------------------------------------------------------------

/**
 * Compute the content-addressable id for an invariant. Two runs that
 * produce the same SMT property + bindings collapse to the same id and
 * therefore the same file on disk. Idempotent.
 *
 * Sort the bindings before hashing so two runs that emit them in
 * different orders still hash the same. The smt assertion + declarations
 * are already canonicalized by C1.
 */
export function hashInvariant(input: {
  smt: StoredInvariant["smt"];
  bindings: StoredInvariant["bindings"];
}): string {
  const sortedBindings = [...input.bindings].sort((a, b) =>
    a.smt_constant.localeCompare(b.smt_constant),
  );
  const payload = JSON.stringify({
    declarations: input.smt.declarations,
    assertion: input.smt.assertion,
    kind: input.smt.kind,
    bindings: sortedBindings.map((b) => {
      if (b.type === "graph") {
        return {
          t: "graph",
          c: b.smt_constant,
          r: b.relation,
          rp: b.root.filePath,
          p: b.predicate,
          pa: b.predicateArg,
        };
      }
      return {
        t: "local",
        c: b.smt_constant,
        e: b.source_expr,
        s: b.sort,
      };
    }),
  });
  return createHash("sha256").update(payload).digest("hex").slice(0, 16);
}

// ---------------------------------------------------------------------------
// Build a StoredInvariant from a fix-loop bundle's parts
// ---------------------------------------------------------------------------

/**
 * Convert the fix loop's in-memory state (InvariantClaim + locus + test +
 * patch sha) into a StoredInvariant ready to write. Pure function;
 * doesn't touch disk.
 *
 * Caller is responsible for resolving each binding's file path + the
 * content hash of the source span. The InvariantClaim's bindings already
 * carry source_line / source_expr; we use those to fill the node block.
 * For v1 we treat one-line bindings as a single-line node range and
 * leave the nodeHash to the caller (the substrate has the AST + content
 * hashes; we don't reach into the substrate from this module to keep
 * the dependency graph clean).
 */
export function buildStoredInvariant(args: {
  claim: InvariantClaim;
  signal: BugSignal;
  locus: BugLocus;
  test: TestArtifact | null;
  patchSha: string | null;
  scope?: "callsite" | "sink";
  /**
   * For each binding's source position, the substrate's content hash for
   * the corresponding AST node. Caller produces this; this module just
   * threads it through.
   */
  bindingNodeHashes: Map<string, string>;
  /**
   * Optional override for the callsite block. The orchestrator's
   * persistence path passes this when C3 produced a patch — Locate's
   * `locus` is a B-stage best guess and frequently differs from the file
   * C3 actually patched. When omitted, falls back to `locus` (preserves
   * existing callers).
   */
  callsiteOverride?: {
    filePath: string;
    startLine: number;
    endLine: number;
  };
  /**
   * Containing function snapshot at write time, for self-healing binding.
   * The caller looks up the function-shaped substrate node that contains
   * the callsite line and passes its `subtreeHash` plus its `startLine`.
   * The mint computes `functionOffset = callsite.startLine - fn.startLine`,
   * which the resolver later uses to recompute the line when the file
   * shifts. Optional for backward compatibility; absent values mean the
   * binding is line-only and cannot self-heal.
   */
  containingFunction?: {
    hash: string;
    startLine: number;
  } | null;
  /**
   * Per-binding location override. Keyed by smt_constant. When a key is
   * present, the binding's `node.filePath` and `node.startLine` /
   * `node.endLine` come from this map instead of `locus.file` and
   * `b.source_line`. When a key is absent, the binding falls back to
   * the legacy locus-derived shape. The orchestrator populates this by
   * locating each binding's `source_expr` text in C3's post-edit
   * `newContent` — a binding whose expression cannot be located gets an
   * explicit `{ startLine: 0, endLine: 0 }` rather than a silent fall-
   * back to the pre-edit line guess.
   */
  bindingLocations?: Map<
    string,
    { filePath: string; startLine: number; endLine: number }
  >;
}): StoredInvariant {
  const {
    claim,
    signal,
    locus,
    test,
    patchSha,
    scope = "callsite",
    bindingNodeHashes,
    callsiteOverride,
    bindingLocations,
    containingFunction,
  } = args;

  const bindings: StoredInvariant["bindings"] = claim.bindings.map((b) => {
    const loc = bindingLocations?.get(b.smt_constant);
    const localBinding: LocalBinding = {
      type: "local",
      smt_constant: b.smt_constant,
      source_expr: b.source_expr,
      sort: b.sort,
      node: {
        filePath: loc ? loc.filePath : locus.file,
        nodeHash: bindingNodeHashes.get(b.smt_constant) ?? "",
        startLine: loc ? loc.startLine : b.source_line,
        endLine: loc ? loc.endLine : b.source_line,
      },
    };
    return localBinding;
  });

  const smt: StoredInvariant["smt"] = {
    kind: normalizeKind(claim.llmKind),
    declarations: parseDeclarations(claim.formalExpression),
    assertion: claim.formalExpression,
  };

  const id = hashInvariant({ smt, bindings });

  const baseLine = callsiteOverride ? callsiteOverride.startLine : locus.line;
  const fnHash = containingFunction?.hash ?? null;
  const fnOffset =
    containingFunction != null ? baseLine - containingFunction.startLine : null;

  const callsite: StoredInvariant["callsite"] = callsiteOverride
    ? {
        filePath: callsiteOverride.filePath,
        function: locus.function ?? null,
        functionHash: fnHash,
        functionOffset: fnOffset,
        startLine: callsiteOverride.startLine,
        endLine: callsiteOverride.endLine,
      }
    : {
        filePath: locus.file,
        function: locus.function ?? null,
        functionHash: fnHash,
        functionOffset: fnOffset,
        startLine: locus.line,
        endLine: locus.line,
      };

  return {
    id,
    createdAt: new Date().toISOString(),
    originatingBug: signal.summary,
    smt,
    bindings,
    callsite,
    scope,
    regressionTest: test
      ? { filePath: test.testFilePath, testName: test.testName }
      : null,
    patchSha,
    retired: null,
  };
}

/**
 * Normalize the LLM-emitted kind string to the spec's enumeration.
 * The InvariantClaim carries `llmKind` as a free-form string per
 * types.ts (the C1 prompt asks for one of six known values, but we
 * defend against drift). Anything outside the known set falls through
 * to "other" — a valid storable kind that routes to the behavioral
 * verification path.
 */
function normalizeKind(llmKind: string | undefined): StoredInvariant["smt"]["kind"] {
  switch (llmKind) {
    case "arithmetic":
    case "set_uniqueness":
    case "cardinality":
    case "order":
    case "taint":
    case "other":
      return llmKind;
    default:
      return "other";
  }
}

/**
 * The InvariantClaim's `formalExpression` field is the assertion text
 * (a single `(assert ...)` line). Declarations are not separately
 * tracked on the claim today — C1 emits them but they get folded into
 * the formalExpression for Z3 sat-checking. For the on-disk format we
 * lift them back out so the standing runtime can re-feed them to Z3
 * along with the rebound bindings.
 *
 * Heuristic extraction: any leading `(declare-...)` lines are
 * declarations; the remainder is the assertion. v1 best-effort; can
 * be tightened when C1 starts emitting declarations as a separate
 * field.
 */
function parseDeclarations(formalExpression: string): string[] {
  const lines = formalExpression.split("\n").map((l) => l.trim()).filter(Boolean);
  const declarations: string[] = [];
  for (const line of lines) {
    if (line.startsWith("(declare-")) {
      declarations.push(line);
    } else {
      break;
    }
  }
  return declarations;
}

// ---------------------------------------------------------------------------
// Disk I/O
// ---------------------------------------------------------------------------

/**
 * Resolve `.provekit/invariants/` under the given project root, creating
 * it if necessary. Idempotent.
 */
export function invariantStoreDir(projectRoot: string): string {
  const dir = join(projectRoot, ".provekit", "invariants");
  mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * Write a StoredInvariant to `.provekit/invariants/<id>.json`. Idempotent:
 * a re-write produces an identical file. The caller is responsible for
 * deciding whether the patch commit and this file should be source-
 * controlled together (they should — but that's a workflow decision,
 * not this module's responsibility).
 */
export function writeInvariant(projectRoot: string, invariant: StoredInvariant): string {
  const dir = invariantStoreDir(projectRoot);
  const path = join(dir, `${invariant.id}.json`);
  writeFileSync(path, JSON.stringify(invariant, null, 2) + "\n", "utf-8");
  return path;
}

/**
 * Read every invariant in the store. Skips retired ones unless
 * `includeRetired` is true. Returns invariants ordered by createdAt
 * ascending (deterministic listing for `provekit verify` output).
 */
export function readInvariants(
  projectRoot: string,
  options: { includeRetired?: boolean } = {},
): StoredInvariant[] {
  const dir = join(projectRoot, ".provekit", "invariants");
  if (!existsSync(dir)) return [];

  const out: StoredInvariant[] = [];
  for (const name of readdirSync(dir)) {
    if (!name.endsWith(".json")) continue;
    const path = join(dir, name);
    let parsed: unknown;
    try {
      parsed = JSON.parse(readFileSync(path, "utf-8"));
    } catch {
      // Corrupt or partial write — skip; the verify CLI surfaces this
      // separately. v1 doesn't need to be aggressive about recovery.
      continue;
    }
    const inv = parsed as StoredInvariant;
    if (!options.includeRetired && inv.retired) continue;
    // Normalize legacy bindings (no type discriminant) to type:"local".
    // Idempotent on already-typed bindings.
    if (Array.isArray(inv.bindings)) {
      inv.bindings = inv.bindings.map((b) => normalizeBinding(b));
    }
    out.push(inv);
  }

  out.sort((a, b) => a.createdAt.localeCompare(b.createdAt));
  return out;
}

/**
 * Mark an invariant as retired. Audit-preserving: the file remains on
 * disk, the `retired` field gets populated. `provekit verify` skips
 * retired invariants by default.
 */
export function retireInvariant(
  projectRoot: string,
  id: string,
  reason: string,
): StoredInvariant | null {
  const dir = join(projectRoot, ".provekit", "invariants");
  const path = join(dir, `${id}.json`);
  if (!existsSync(path)) return null;

  const parsed = JSON.parse(readFileSync(path, "utf-8")) as StoredInvariant;
  if (parsed.retired) return parsed; // idempotent

  parsed.retired = {
    at: new Date().toISOString(),
    reason,
  };
  writeFileSync(path, JSON.stringify(parsed, null, 2) + "\n", "utf-8");
  return parsed;
}
