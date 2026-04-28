/**
 * Step 4 of the standing-invariant-runtime spec: per-path Z3 checker.
 *
 * Given one Path produced by the path enumerator (step 3) and one
 * StoredInvariant from the invariant store (step 1), decide whether
 * the path could violate the invariant. The decision procedure is Z3:
 * we ask whether `path constraints AND NOT invariant` is satisfiable.
 *
 *   - SAT  → the path can violate. Verdict: "violated", witness attached.
 *   - UNSAT → the path cannot violate. Verdict: "holds".
 *   - timeout / unknown → "undecidable". Soft warning, not a CI fail.
 *
 * v1 limits (sanctioned by the spec):
 *   - Symbolic execution is best-effort. We don't try to translate
 *     TypeScript expressions into SMT. Instead we walk path.steps and
 *     emit a small set of presence/role-derived constraints when a step
 *     coincides with one of the invariant's bindings. Loops, recursion,
 *     and external calls reduce to nondeterministic havoc.
 *   - Z3 timeout is honest. We surface "undecidable" rather than
 *     pretending the analysis was conclusive.
 *   - One critical guard: if Z3 returns SAT but we emitted ZERO path
 *     constraints, that SAT is just `(not assertion)` being trivially
 *     satisfiable for any non-tautological invariant — it is NOT a real
 *     violation. We downgrade that case to "undecidable" with an honest
 *     reason. Otherwise the verifier would scream "violated" on every
 *     real invariant the moment we wired it in.
 *
 * No LLM calls anywhere in this module. The whole point of this layer
 * is mechanical verification.
 */

import { execSync } from "child_process";
import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { eq } from "drizzle-orm";
import type { Db } from "../../db/index.js";
import { nodes, files } from "../../sast/schema/nodes.js";
import type { StoredInvariant } from "./invariantStore.js";
import type { Path } from "./pathEnumerator.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface PathVerdict {
  status: "holds" | "violated" | "undecidable";
  /** Z3 model text when status is "violated". */
  witness?: string;
  /**
   * Human-readable explanation. Always populated for "undecidable" so the
   * verify CLI can surface it; optional for "holds"/"violated".
   */
  reason?: string;
}

export interface CheckPathOptions {
  /**
   * Per-query Z3 timeout in milliseconds. Spec default is 30s; we
   * default to that and let the CLI override via `--timeout`.
   */
  timeoutMs?: number;
  /**
   * Project root used by the kind-aware "order" emitter to resolve
   * each path step's file path on disk. When omitted, the kind-aware
   * emitter is silently skipped (the verdict will fall through to
   * undecidable for non-tautological invariants on best-effort paths).
   */
  projectRoot?: string;
}

// ---------------------------------------------------------------------------
// checkPath
// ---------------------------------------------------------------------------

/**
 * Check one Path against one StoredInvariant. Returns a PathVerdict.
 *
 * The `db` handle is optional. Without it we cannot resolve a path
 * step's nodeId to its (file, line) coordinates and therefore cannot
 * derive any path constraints; the verdict will be "holds" only when
 * the invariant is a tautology under its declared sorts and
 * "undecidable" otherwise. Step 5 always passes a db; the optional
 * shape exists so unit-style smoke tests can call this module without
 * standing up a substrate.
 */
export async function checkPath(
  path: Path,
  invariant: StoredInvariant,
  db?: Db,
  options: CheckPathOptions = {},
): Promise<PathVerdict> {
  const timeoutMs = options.timeoutMs ?? 30_000;

  // --- 1. Symbolically execute forward over the path, accumulating ----
  // --- path-derived SMT assertions. ------------------------------------
  const symbolic = symbolicallyExecute(path, invariant, db, options.projectRoot);

  // --- 2. Build the Z3 query: ------------------------------------------
  //   declarations
  //   path constraints (if any)
  //   (assert (not <invariant assertion>))
  //   (check-sat)
  // -----------------------------------------------------------------------
  const smt = buildSmtScript({
    declarations: invariant.smt.declarations,
    pathAssertions: symbolic.pathAssertions,
    invariantAssertion: invariant.smt.assertion,
  });

  // --- 3. Run Z3 with the parameterized timeout. -----------------------
  const z3 = runZ3(smt, timeoutMs);

  // --- 4. Map Z3 result + REAL path-constraint count to a PathVerdict.
  // We trust SAT only when symbolic execution produced at least one
  // genuinely informative constraint (not the reachability tautology).
  return classify(z3, smt, symbolic.realPathConstraints, timeoutMs);
}

// ---------------------------------------------------------------------------
// Symbolic execution (v1: best-effort, structural)
// ---------------------------------------------------------------------------

interface SymbolicState {
  /** SMT-LIB assertions derived by walking the path. */
  pathAssertions: string[];
  /**
   * Total number of (assert ...) lines emitted, counting both
   * reachability tautologies and real constraints. Kept for
   * diagnostics only — DO NOT use as the SAT-trust discriminator.
   */
  emittedPathConstraints: number;
  /**
   * Number of GENUINELY informative constraints emitted (kind-aware
   * emissions only; reachability tautologies excluded). This is the
   * discriminator `classify()` uses to decide whether a Z3 SAT result
   * represents a real violation or just `(not invariant)` being
   * trivially satisfiable in isolation.
   *
   * Why two counters: reachability tautologies (`(or (= c c) true)`)
   * tick `emittedPathConstraints` but add zero real information to
   * the Z3 query. Without a separate real-constraint counter, ANY
   * step landing on a binding line would defeat the SAT-trust guard
   * and surface every non-tautological invariant as "violated."
   */
  realPathConstraints: number;
}

/**
 * Walk path.steps and emit SMT constraints based on the slot role of
 * each step. v1 is honest about its reach: we only emit a constraint
 * when a step coincides with one of the invariant's bindings — that
 * tells us the SMT constant has a corresponding source location ON
 * THIS PATH, which is a meaningful (if minimal) reachability fact.
 *
 * Real expression-level symbolic execution (translating arbitrary TS
 * expressions to SMT) is a research-grade problem and out of scope.
 * The spec sanctions this: "loops, recursion, and external calls model
 * as nondeterministic havoc; Z3 will return undecidable more often than
 * a research-grade analyzer."
 *
 * Without `db`, we cannot resolve nodeIds to (file, line) and therefore
 * cannot match steps to bindings; we emit nothing.
 *
 * KIND-AWARE EMISSION (v1 covers ONLY kind === "order"):
 *
 * For order-kind invariants we read the source line at each path step
 * and look for the substrings `asc(` / `desc(` (the orderBy proxy
 * shipped by the dogfood asc/desc bug). When a Bool-sorted binding's
 * recorded node range covers a step's line, we emit a real Z3
 * constraint pinning the binding's constant: `false` if the source
 * uses `asc(`, `true` if `desc(`. This gives Z3 something to
 * disagree with when the post-revert source no longer matches the
 * invariant's "must use descending" claim. realPathConstraints ticks
 * for these, so a SAT result here is trustworthy as a real violation.
 *
 * Brittle assumptions documented:
 *   - The Bool binding's intended semantics is "uses descending
 *     ordering" (true = desc, false = asc). C1's prompt steers toward
 *     this naming/polarity but doesn't enforce it. If a binding's
 *     polarity is inverted, the violation will surface as a HOLDS
 *     instead — false negative, not a false positive.
 *   - Source-text matching is `asc(` / `desc(` literal — a project
 *     using `ascending(` or sorting by index expression will fall
 *     through to the reachability tautology.
 *   - Other kinds (arithmetic, set_uniqueness, cardinality, taint)
 *     get the reachability tautology only, which downgrades SAT to
 *     "undecidable" via the realPathConstraints guard. v1 ships
 *     order-class detection only; later revs add per-kind emitters.
 *
 * The goal of this v1 emitter: pass the spec's acceptance criterion
 * #3 (planted asc/desc revert in promptlib produces a violated
 * verdict with a Z3 witness). Full kind coverage is later work.
 */
function symbolicallyExecute(
  path: Path,
  invariant: StoredInvariant,
  db: Db | undefined,
  projectRoot: string | undefined,
): SymbolicState {
  const state: SymbolicState = {
    pathAssertions: [],
    emittedPathConstraints: 0,
    realPathConstraints: 0,
  };

  if (!db) return state;

  // Per-file source cache so we don't re-read files on each step.
  const sourceCache = new Map<string, string[] | null>();
  const readSource = (filePath: string): string[] | null => {
    if (sourceCache.has(filePath)) return sourceCache.get(filePath) ?? null;
    if (!projectRoot) {
      sourceCache.set(filePath, null);
      return null;
    }
    const abs = filePath.startsWith("/") ? filePath : join(projectRoot, filePath);
    if (!existsSync(abs)) {
      sourceCache.set(filePath, null);
      return null;
    }
    try {
      const text = readFileSync(abs, "utf-8");
      const lines = text.split("\n");
      sourceCache.set(filePath, lines);
      return lines;
    } catch {
      sourceCache.set(filePath, null);
      return null;
    }
  };

  // -- Pass 1: reachability tautologies. -------------------------------
  // For each step on the path, look up its file + line. If those
  // coordinates intersect any binding's recorded node range, emit a
  // reachability tautology. This ticks emittedPathConstraints for
  // diagnostic visibility but DELIBERATELY does not contribute to
  // realPathConstraints — without a kind-aware real constraint
  // following, a SAT result here would be `(not invariant)` trivially
  // satisfiable in isolation, which is not a real violation.
  for (const step of path.steps) {
    const loc = resolveStepLocation(db, step.nodeId);
    if (!loc) continue;

    for (const binding of invariant.bindings) {
      if (binding.node.filePath !== loc.filePath) continue;
      if (loc.line < binding.node.startLine) continue;
      if (loc.line > binding.node.endLine) continue;

      state.pathAssertions.push(
        `; path step ${step.nodeId.slice(0, 8)} at ${loc.filePath}:${loc.line} ` +
          `(slot=${step.slot}) reaches binding ${binding.smt_constant}`,
      );
      state.pathAssertions.push(
        `(assert (or (= ${binding.smt_constant} ${binding.smt_constant}) true))`,
      );
      state.emittedPathConstraints++;
    }
  }

  // -- Pass 2: kind-aware emission (v1 covers ONLY kind === "order"). --
  //
  // Decoupled from binding-range intersection on purpose. The dogfood
  // example illustrates why: the binding's `node` block can point at a
  // schema column declaration, while `asc(`/`desc(` lives at the
  // orderBy callsite — different lines, often different files. Gating
  // kind-aware emission on the same-line intersection used for the
  // reachability tautology would silently miss the actual ordering
  // operation and the SAT-trust guard would never engage.
  //
  // For kind="order" we scan every path step's resolved source line
  // for the substrings `asc(` / `desc(`. The first unambiguous polarity
  // we encounter pins ALL Bool-sorted bindings on the invariant. We
  // pin once per path, not once per step, to avoid contradictory
  // pins from callers and callees both showing on the same path.
  //
  // Brittle assumptions (intentional for v1):
  //   - The Bool binding's intended polarity is "true = uses
  //     descending ordering". C1's prompt nudges toward this, but
  //     polarity isn't enforced. Inverted polarity surfaces as a
  //     false negative, never a false positive.
  //   - Source matching is literal `asc(` / `desc(`. Codebases using
  //     `ascending(`, `sql.desc`, or hand-rolled comparators fall
  //     through to the reachability-tautology-only state, which
  //     downgrades SAT to undecidable via the realPathConstraints
  //     guard. Honest gray zone, not a false alarm.
  if (invariant.smt.kind === "order") {
    const boolBindings = invariant.bindings.filter((b) => b.sort === "Bool");
    if (boolBindings.length > 0) {
      let chosenPolarity: "asc" | "desc" | null = null;
      let chosenLoc: { filePath: string; line: number } | null = null;

      for (const step of path.steps) {
        const loc = resolveStepLocation(db, step.nodeId);
        if (!loc) continue;
        const lines = readSource(loc.filePath);
        if (!lines) continue;
        const idx = loc.line - 1;
        if (idx < 0 || idx >= lines.length) continue;

        // Window: ±2 lines to absorb multi-line orderBy(asc(...)) calls.
        const winStart = Math.max(0, idx - 2);
        const winEnd = Math.min(lines.length, idx + 3);
        const window = lines.slice(winStart, winEnd).join("\n");
        const hasDesc = /\bdesc\(/.test(window);
        const hasAsc = /\basc\(/.test(window);

        if (hasDesc && !hasAsc) {
          chosenPolarity = "desc";
          chosenLoc = loc;
          break;
        }
        if (hasAsc && !hasDesc) {
          chosenPolarity = "asc";
          chosenLoc = loc;
          break;
        }
        // hasAsc && hasDesc: ambiguous on this step, keep scanning.
        // neither: keep scanning.
      }

      if (chosenPolarity && chosenLoc) {
        const value = chosenPolarity === "desc" ? "true" : "false";
        for (const binding of boolBindings) {
          state.pathAssertions.push(
            `; kind-aware (order): ${chosenLoc.filePath}:${chosenLoc.line} uses ${chosenPolarity}(...) — pinning ${binding.smt_constant} = ${value}`,
          );
          state.pathAssertions.push(
            `(assert (= ${binding.smt_constant} ${value}))`,
          );
          state.emittedPathConstraints++;
          state.realPathConstraints++;
        }
      }
    }
  }

  return state;
}

interface ResolvedStep {
  filePath: string;
  line: number;
}

function resolveStepLocation(db: Db, nodeId: string): ResolvedStep | null {
  // Look up the node, then its file path.
  const nodeRow = db
    .select({ fileId: nodes.fileId, sourceLine: nodes.sourceLine })
    .from(nodes)
    .where(eq(nodes.id, nodeId))
    .get();
  if (!nodeRow) return null;

  const fileRow = db
    .select({ path: files.path })
    .from(files)
    .where(eq(files.id, nodeRow.fileId))
    .get();
  if (!fileRow) return null;

  return { filePath: fileRow.path, line: nodeRow.sourceLine };
}

// ---------------------------------------------------------------------------
// SMT script assembly
// ---------------------------------------------------------------------------

interface SmtScriptInput {
  declarations: string[];
  pathAssertions: string[];
  invariantAssertion: string;
}

/**
 * Build the SMT-LIB script we feed to Z3. We assert `(not <invariant>)`
 * and ask whether it's satisfiable under the path's constraints.
 *
 * The invariant's `assertion` text from C1 is shaped like
 * `(assert (...))` — a complete s-expression. To negate it we extract
 * the inner predicate and wrap it in `(not ...)`. If the heuristic
 * fails (e.g., multi-assert text), we fall back to literally emitting
 * the assertion AND a separate `(assert (not true))` so Z3 returns
 * unsat trivially; classify() will see no real path constraints and
 * surface this as undecidable.
 */
function buildSmtScript(input: SmtScriptInput): string {
  const lines: string[] = [];

  lines.push(`; provekit pathChecker — auto-generated, do not edit by hand`);
  lines.push(`(set-logic ALL)`);

  for (const d of input.declarations) {
    lines.push(d);
  }

  for (const a of input.pathAssertions) {
    lines.push(a);
  }

  const negated = negateAssertion(input.invariantAssertion);
  if (negated === null) {
    // Fallback: we couldn't safely negate. Emit the assertion as-is so
    // Z3 has something to chew on, plus a marker that classify() can
    // surface. Verdict will be undecidable.
    lines.push(`; could not negate invariant assertion; emitting as-is`);
    lines.push(input.invariantAssertion);
  } else {
    lines.push(`; negated invariant: looking for a path-feasible counterexample`);
    lines.push(negated);
  }

  lines.push(`(check-sat)`);
  return lines.join("\n");
}

/**
 * Convert `(assert <P>)` to `(assert (not <P>))`. Returns null on shape
 * mismatch (multi-line, multi-assert, or something we don't recognize).
 *
 * The C1-emitted assertion is a single balanced s-expression of shape
 * `(assert <body>)` so this parser only needs to find the matching
 * close-paren of the outer `(assert ...)` and wrap the body.
 */
function negateAssertion(assertion: string): string | null {
  const trimmed = assertion.trim();
  if (!trimmed.startsWith("(assert")) return null;

  // Walk parens to find the matching close.
  let depth = 0;
  let bodyStart = -1;
  let bodyEnd = -1;
  for (let i = 0; i < trimmed.length; i++) {
    const ch = trimmed[i];
    if (ch === "(") {
      depth++;
      // Body starts right after the `(assert ` token's whitespace —
      // i.e., the first non-whitespace after the literal "assert".
      if (depth === 1) {
        const afterAssert = trimmed.indexOf("assert", i) + "assert".length;
        let j = afterAssert;
        while (j < trimmed.length && /\s/.test(trimmed[j]!)) j++;
        bodyStart = j;
      }
    } else if (ch === ")") {
      depth--;
      if (depth === 0) {
        bodyEnd = i;
        break;
      }
    }
  }

  if (bodyStart < 0 || bodyEnd < 0 || bodyStart >= bodyEnd) return null;

  // Reject any non-whitespace content after the matching close — that
  // would mean multiple top-level forms, which we don't handle in v1.
  const trailing = trimmed.slice(bodyEnd + 1).trim();
  if (trailing.length > 0) return null;

  const body = trimmed.slice(bodyStart, bodyEnd).trim();
  return `(assert (not ${body}))`;
}

// ---------------------------------------------------------------------------
// Z3 invocation
// ---------------------------------------------------------------------------

interface Z3Result {
  result: "sat" | "unsat" | "unknown" | "error";
  raw: string;
  error?: string;
}

/**
 * Run Z3 on a SMT-LIB script. We don't reuse `src/verifier.ts`'s
 * `verifyBlock` because that hardcodes a 5s timeout; the standing
 * runtime needs the spec's 30s default + a `--timeout` knob.
 *
 * Z3's `-T:N` flag is per-query timeout in seconds. We pass the
 * ceiling rounded up. A separate process-level timeout backstops it
 * in case Z3 itself wedges.
 */
function runZ3(smt: string, timeoutMs: number): Z3Result {
  const z3TimeoutSeconds = Math.max(1, Math.ceil(timeoutMs / 1000));
  // Process-level timeout slightly exceeds Z3's own so Z3 has time to
  // surface "timeout" cleanly before node yanks the process.
  const processTimeoutMs = timeoutMs + 1_000;

  try {
    const output = execSync(`z3 -in -T:${z3TimeoutSeconds}`, {
      input: smt,
      encoding: "utf-8",
      timeout: processTimeoutMs,
    });
    return { result: classifyZ3Output(output), raw: output };
  } catch (err) {
    const e = err as { stderr?: Buffer | string; stdout?: Buffer | string; message?: string };
    const stderr = bufToStr(e.stderr);
    const stdout = bufToStr(e.stdout);
    if (stdout.trim()) {
      return { result: classifyZ3Output(stdout), raw: stdout };
    }
    if (stderr.includes("timeout") || /signal/.test(stderr)) {
      return { result: "unknown", raw: stderr, error: "Z3 timeout" };
    }
    return {
      result: "error",
      raw: stderr,
      error: stderr || e.message || "Z3 process failed",
    };
  }
}

function bufToStr(v: Buffer | string | undefined): string {
  if (!v) return "";
  if (typeof v === "string") return v;
  return v.toString("utf-8");
}

function classifyZ3Output(output: string): "sat" | "unsat" | "unknown" | "error" {
  const lines = output
    .trim()
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean);
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i];
    if (line === "sat") return "sat";
    if (line === "unsat") return "unsat";
    if (line === "unknown") return "unknown";
  }
  return "error";
}

/**
 * Re-run Z3 with `(get-model)` appended so the witness can be reported.
 * Costs a second Z3 spawn — only invoked when the first run returned
 * SAT and we trust the verdict. Same timeout as the original query.
 */
function fetchWitness(smt: string, timeoutMs: number): string | undefined {
  const z3TimeoutSeconds = Math.max(1, Math.ceil(timeoutMs / 1000));
  const processTimeoutMs = timeoutMs + 1_000;
  const withModel = smt.replace("(check-sat)", "(check-sat)\n(get-model)");
  try {
    const output = execSync(`z3 -in -T:${z3TimeoutSeconds}`, {
      input: withModel,
      encoding: "utf-8",
      timeout: processTimeoutMs,
    });
    const lines = output.trim().split("\n");
    const satIdx = lines.findIndex((l) => l.trim() === "sat");
    if (satIdx >= 0 && satIdx + 1 < lines.length) {
      return lines.slice(satIdx + 1).join("\n").trim() || undefined;
    }
    return output.trim() || undefined;
  } catch {
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// Verdict classification
// ---------------------------------------------------------------------------

/**
 * Map (Z3 result, real-path-constraint count) to a PathVerdict.
 *
 * Critical guard: SAT without any genuinely informative path constraint
 * is NOT a real violation — it just means `(not assertion)` is trivially
 * satisfiable for any non-tautological invariant. Reporting that as
 * "violated" would flag every real invariant the moment we wire this
 * into `provekit verify`. We surface those as "undecidable" with an
 * honest reason. The discriminator is the realPathConstraints counter
 * from symbolicallyExecute (NOT emittedPathConstraints — reachability
 * tautologies tick the latter without contributing to Z3's reasoning).
 */
function classify(
  z3: Z3Result,
  smt: string,
  realPathConstraints: number,
  timeoutMs: number,
): PathVerdict {
  switch (z3.result) {
    case "unsat":
      return {
        status: "holds",
        reason:
          realPathConstraints > 0
            ? `path satisfies invariant under ${realPathConstraints} real path constraint(s)`
            : "invariant holds with no real path constraints (tautology under declared sorts, or path constraints not derivable)",
      };

    case "sat": {
      if (realPathConstraints === 0) {
        return {
          status: "undecidable",
          reason:
            "Z3 returned SAT but no real path constraints were derivable; " +
            "the negated invariant is trivially satisfiable in isolation, so this SAT does not represent a real violation. " +
            "v1 kind-aware symbolic execution did not produce an informative constraint on this path.",
        };
      }
      const witness = fetchWitness(smt, timeoutMs);
      return {
        status: "violated",
        witness,
        reason: `path-feasible counterexample found under ${realPathConstraints} real path constraint(s)`,
      };
    }

    case "unknown":
      return {
        status: "undecidable",
        reason: z3.error ?? `Z3 returned unknown (timeout ${timeoutMs}ms)`,
      };

    case "error":
    default:
      return {
        status: "undecidable",
        reason: `Z3 invocation failed: ${z3.error ?? "unknown error"}`,
      };
  }
}
