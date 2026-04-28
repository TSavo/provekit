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
 * KIND-AWARE EMISSION:
 *
 * For each kind we extract kind-specific evidence from path source
 * lines and pin the invariant's bindings against actual code
 * structure. Each pinning ticks realPathConstraints, gating SAT-trust.
 *
 * Coverage and confidence (v1):
 *
 *   - kind="order"        — CONFIDENT. Scans for `asc(` / `desc(`
 *     literals and pins Bool bindings. The motivating dogfood case.
 *
 *   - kind="cardinality"  — CONFIDENT for "must run / fire / be called"
 *     shapes. Walks the path looking for a call to `<source_expr>(`,
 *     reads polarity from the Bool binding's constant name, pins on
 *     direct presence-evidence only (absence is too weak on a
 *     best-effort path).
 *
 *   - kind="taint"        — CONFIDENT for sanitization-presence. Pins
 *     Bool sanitizer bindings to true when a recognized sanitizer
 *     (escapeHtml, sanitize*, escapeShell, parameterize, etc.)
 *     appears on the path. Absence does NOT pin to false (most safe
 *     paths simply lack a literal sanitizer call); the bug only
 *     surfaces post-fix when a sanitizer is removed and Z3's negated
 *     invariant becomes path-feasible.
 *
 *   - kind="set_uniqueness" — CONSERVATIVE. Fires only on a literal
 *     array containing a syntactic duplicate string entry on a path
 *     step's source line. Pins two Int bindings equal so Z3 sees the
 *     duplicate state. All other set-uniqueness shapes punt to
 *     undecidable. Documented in the per-kind block.
 *
 *   - kind="arithmetic"   — PUNTED for v1. The canonical Int-binding
 *     shape `(assert (= b 0))` requires real symbolic execution to
 *     contradict from a non-equality guard. We detect guards as a
 *     scaffolding signal (comment marker only) but emit no real
 *     constraint. Verdict surfaces as undecidable — honest, matches
 *     the spec's v1 limitation language. Full arithmetic SE is later
 *     work.
 *
 * Brittle assumptions, all kinds:
 *   - Source-text scanning is regex over a ±2 line window. Codebases
 *     that pass through unrecognized helpers (`ascending(`, custom
 *     sanitizers, hand-rolled comparators) fall through to the
 *     reachability-tautology-only state, which downgrades SAT to
 *     undecidable via the realPathConstraints guard. Honest gray
 *     zone, not a false alarm.
 *   - Bool binding polarity comes from constant-name heuristics
 *     (`_at_least_once`, `_ran`, `sanitiz`, etc.). Inverted polarity
 *     surfaces as a HOLDS / undecidable, NEVER as a false-positive
 *     violation.
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

  // -- Pass 2 (kind === "cardinality") --------------------------------
  //
  // Canonical SMT shape (per formulateInvariant.ts prompt):
  //   (declare-const x_ran_at_least_once Bool)
  //   (assert (= x_ran_at_least_once false))
  // The Bool predicate name CARRIES the cardinality relation; the
  // emitter's job is to decide whether the operation actually ran on
  // this path and pin the binding accordingly.
  //
  // Heuristic (best-effort, documented as such):
  //   1. Treat each Bool binding's `source_expr` as a token name (the
  //      operation we're counting). Strip non-identifier chars.
  //   2. Walk path steps; if any step's source line contains the token
  //      followed by `(` (a call), the operation ran on this path.
  //   3. Pin the binding's polarity from its constant name:
  //        - contains "_at_least_once" / "_ran" / "_fired" → ran=true
  //        - contains "_never" / "_zero" / "_not_fired"   → ran=false
  //   4. If we cannot infer polarity from the name, leave UNPINNED —
  //      the SAT-trust guard then surfaces undecidable rather than
  //      guess. Z3 has no other anchor.
  //
  // What this catches confidently:
  //   - "must run at least once" with a `_ran_at_least_once` Bool, where
  //     the operation appears as a call literally on the path.
  //   - "must fire exactly once" / "must not fire" with similarly-named
  //     Bools — same call-presence detection.
  //
  // What this MISSES on purpose:
  //   - Indirect calls (operation invoked via dynamic dispatch / event
  //     bus). Path steps still contain the dispatch frame's source line,
  //     not the dispatched handler's, so a textual scan won't find the
  //     handler name there.
  //   - "Exactly N" cardinality where N > 1: we don't count, we just
  //     witness presence/absence. v1 limitation, sanctioned by the spec
  //     ("loops, recursion, and external calls model as nondeterministic
  //     havoc").
  if (invariant.smt.kind === "cardinality") {
    const boolBindings = invariant.bindings.filter((b) => b.sort === "Bool");
    for (const binding of boolBindings) {
      const polarity = inferCardinalityPolarity(binding.smt_constant);
      if (polarity === null) continue; // can't read the binding's intent

      const token = binding.source_expr.replace(/[^A-Za-z0-9_$]/g, "");
      if (!token || token.length < 2) continue; // too generic to be safe

      let opAppeared = false;
      let chosenLoc: { filePath: string; line: number } | null = null;
      // Compile a word-boundary call regex once per binding.
      const reEsc = token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      const callRe = new RegExp(`\\b${reEsc}\\s*\\(`);

      for (const step of path.steps) {
        const loc = resolveStepLocation(db, step.nodeId);
        if (!loc) continue;
        const lines = readSource(loc.filePath);
        if (!lines) continue;
        const idx = loc.line - 1;
        if (idx < 0 || idx >= lines.length) continue;
        const winStart = Math.max(0, idx - 2);
        const winEnd = Math.min(lines.length, idx + 3);
        const windowText = lines.slice(winStart, winEnd).join("\n");
        if (callRe.test(windowText)) {
          opAppeared = true;
          chosenLoc = loc;
          break;
        }
      }

      // polarity tells us what `true` means for this binding:
      //   - "ran"  → opAppeared==true  ⇒ binding=true
      //   - "never" → opAppeared==false ⇒ binding=true (the absence IS
      //     what the predicate asserts)
      // For honesty we only pin when we have direct evidence (the call
      // appeared) or the path is short enough that absence is a
      // meaningful signal. v1 conservative cut: only pin on PRESENCE.
      // Absence-as-evidence on a partial best-effort path is too weak.
      if (opAppeared && chosenLoc) {
        const value = polarity === "ran" ? "true" : "false";
        state.pathAssertions.push(
          `; kind-aware (cardinality): ${chosenLoc.filePath}:${chosenLoc.line} calls ${token}(...) — pinning ${binding.smt_constant} = ${value} (polarity=${polarity})`,
        );
        state.pathAssertions.push(
          `(assert (= ${binding.smt_constant} ${value}))`,
        );
        state.emittedPathConstraints++;
        state.realPathConstraints++;
      }
    }
  }

  // -- Pass 2 (kind === "taint") --------------------------------------
  //
  // Canonical SMT shape:
  //   (declare-const input_contains_shell_metachar Bool)
  //   (declare-const input_was_sanitized Bool)
  //   (assert (and input_contains_shell_metachar (not input_was_sanitized)))
  //
  // Strategy: identify a sanitization step on the path and pin
  // `input_was_sanitized = true`. The other binding
  // (`input_contains_shell_metachar` or similar) is a value-state
  // property of the data, NOT structural to the path; we leave it
  // unpinned (let Z3 stay undecidable on that axis). The
  // sanitization-presence check is what makes the post-fix verdict
  // possible: when sanitization disappears from the path, this binding
  // pins to false and Z3 SAT becomes a real violation.
  //
  // Sanitizer tokens (case-insensitive substring match in source line):
  //   escapeHtml, escapeShell, sanitize, escapeRegex, parameterize,
  //   bindParam, prepared, $1 / $2 (placeholder param syntax).
  //
  // Conservative: we only emit when we find a sanitizer; absence does
  // NOT pin to false here, because most safe paths simply don't have
  // a literal sanitization call (e.g., the data is intrinsically safe
  // — int-typed param, allowlisted at the boundary). An absence-pin
  // would generate false positives wholesale on those paths.
  //
  // Brittle assumptions:
  //   - Bool binding name semantics: anything matching /sanitiz/ in
  //     the constant name is treated as the sanitization predicate
  //     (true = sanitized).
  //   - Heuristic sanitizer list is non-exhaustive. Custom sanitizers
  //     fall through; verdict surfaces as undecidable for those, not
  //     as false-positive violation.
  if (invariant.smt.kind === "taint") {
    const sanitizerBindings = invariant.bindings.filter(
      (b) => b.sort === "Bool" && /sanitiz|escap|safe|clean/i.test(b.smt_constant),
    );
    if (sanitizerBindings.length > 0) {
      const sanitizerRe = /\b(escapeHtml|escapeShell|escapeRegex|sanitize\w*|escapeJson|encodeURI(Component)?|bindParam|prepared|parameterize\w*)\s*\(/i;
      let foundSanitizer: { filePath: string; line: number } | null = null;
      for (const step of path.steps) {
        const loc = resolveStepLocation(db, step.nodeId);
        if (!loc) continue;
        const lines = readSource(loc.filePath);
        if (!lines) continue;
        const idx = loc.line - 1;
        if (idx < 0 || idx >= lines.length) continue;
        const winStart = Math.max(0, idx - 2);
        const winEnd = Math.min(lines.length, idx + 3);
        const windowText = lines.slice(winStart, winEnd).join("\n");
        if (sanitizerRe.test(windowText)) {
          foundSanitizer = loc;
          break;
        }
      }
      if (foundSanitizer) {
        for (const binding of sanitizerBindings) {
          state.pathAssertions.push(
            `; kind-aware (taint): ${foundSanitizer.filePath}:${foundSanitizer.line} calls a recognized sanitizer — pinning ${binding.smt_constant} = true`,
          );
          state.pathAssertions.push(
            `(assert (= ${binding.smt_constant} true))`,
          );
          state.emittedPathConstraints++;
          state.realPathConstraints++;
        }
      }
      // No sanitizer found: deliberately leave bindings unpinned.
      // See the block comment above for why absence-as-evidence is
      // unsafe here.
    }
  }

  // -- Pass 2 (kind === "arithmetic") ---------------------------------
  //
  // Canonical SMT shape:
  //   (declare-const b Int)
  //   (assert (= b 0))
  //
  // The bindings are Int-sorted, not Bool. To make Z3's "negate the
  // invariant and check SAT" return a USEFUL verdict, we'd have to
  // pin the Int constant to a value that either matches or contradicts
  // the assertion's hypothesis. That's a research-grade problem in
  // general (it requires real symbolic execution of the path's
  // arithmetic). For v1 we instead emit GUARD evidence: when a
  // structural guard against the bug shape exists on the path
  // (`if (b !== 0)` for a divide-by-zero invariant), Z3 should be
  // told the path is safe.
  //
  // Conservative cut: we look for the binding's source_expr appearing
  // in a guard-like construct on the path. If found, we emit a comment
  // marker only (no constraint), because we cannot soundly contradict
  // the invariant's `(= b 0)` hypothesis from a non-equality guard
  // without proper SE. We DO NOT emit a "violated" constraint; the
  // arithmetic emitter is intentionally conservative — false negatives
  // (verdict: undecidable) are preferred over false positives.
  //
  // What this means in practice:
  //   - Arithmetic invariants currently surface as "undecidable" on
  //     most paths. That is honest and matches the spec's v1
  //     limitation language. A future iteration will translate
  //     literal arithmetic operators to SMT for real symbolic
  //     execution.
  //   - We keep the guard-detection scaffolding in code so the next
  //     iteration has a documented anchor.
  //
  // PUNTED: full arithmetic SE is out of scope for v1. Documented in
  // the block comment above the pass-2 dispatcher.
  if (invariant.smt.kind === "arithmetic") {
    const intBindings = invariant.bindings.filter((b) => b.sort === "Int" || b.sort === "Real");
    for (const binding of intBindings) {
      const token = binding.source_expr.replace(/[^A-Za-z0-9_$]/g, "");
      if (!token || token.length < 1) continue;
      const reEsc = token.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      // Detect guards: `if (b !== 0)`, `if (b)`, `b == 0 ?`, `b === 0 ?`,
      // `b !== 0 ?`. Conservative: if the guard literally contradicts
      // the invariant's `(= b 0)` hypothesis on this path, leave a
      // comment marker but do NOT emit a constraint. v1 punts here.
      const guardRe = new RegExp(
        `\\bif\\s*\\([^)]*\\b${reEsc}\\b[^)]*\\)|\\b${reEsc}\\b\\s*(===|!==|==|!=)\\s*0\\s*\\?`,
      );
      let foundGuard: { filePath: string; line: number } | null = null;
      for (const step of path.steps) {
        const loc = resolveStepLocation(db, step.nodeId);
        if (!loc) continue;
        const lines = readSource(loc.filePath);
        if (!lines) continue;
        const idx = loc.line - 1;
        if (idx < 0 || idx >= lines.length) continue;
        const winStart = Math.max(0, idx - 2);
        const winEnd = Math.min(lines.length, idx + 3);
        const windowText = lines.slice(winStart, winEnd).join("\n");
        if (guardRe.test(windowText)) {
          foundGuard = loc;
          break;
        }
      }
      if (foundGuard) {
        // Comment-only — see block comment above for why we do not
        // emit a hard constraint here in v1. Does NOT tick
        // realPathConstraints.
        state.pathAssertions.push(
          `; kind-aware (arithmetic): guard detected against ${token} at ${foundGuard.filePath}:${foundGuard.line}; v1 emits no constraint (full SE punted) — verdict will be undecidable`,
        );
        state.emittedPathConstraints++;
      }
    }
  }

  // -- Pass 2 (kind === "set_uniqueness") -----------------------------
  //
  // Canonical SMT shapes:
  //   - distinct form:    (assert (not (distinct m1 m2 m3)))
  //   - paired-equality:  (assert (= m1 m2))
  // Bindings are Int-sorted; each `mN` represents the value of the
  // N-th element in the should-be-set.
  //
  // Decisive evidence we trust:
  //   - A literal ARRAY containing a syntactic duplicate, e.g.
  //     `["GET", "POST", "GET"]`. We extract string-literal entries
  //     from the line and check for duplicates.
  // What we do with that evidence:
  //   - Emit `(assert (= m1 m2))` for the first two Int bindings, so
  //     Z3 sees the duplicate state and the invariant's
  //     `(not (distinct ...))` becomes consistent (i.e., the negated
  //     invariant `(distinct ...)` becomes UNSAT under the equality
  //     pin, surfacing "holds" if the invariant is "must be unique").
  //     Wait — re-check: the C1 invariant ASSERTS the violation:
  //     `(not (distinct ...))` is the canonical form, meaning Z3 SAT
  //     is the bug shape. We negate that to `(distinct ...)`, then
  //     pin `m1=m2` from the literal duplicate → negated invariant
  //     UNSAT → verdict "violated" via the path checker's classify().
  //     That matches the spec.
  //
  // What we DON'T trust (sanctioned punts, documented):
  //   - `array.push(x)` without preceding `array.includes(x)` —
  //     dataflow-level analysis required, full SE punt.
  //   - Object literals with duplicate keys: TS catches statically;
  //     scanning runtime paths for them is noisy.
  //   - Any non-literal duplicate (computed dup, .concat with
  //     overlapping arrays). Falls through to undecidable.
  //
  // Conservative: only fires on a literal-array dup found verbatim on
  // a path step's source line.
  if (invariant.smt.kind === "set_uniqueness") {
    const intBindings = invariant.bindings.filter((b) => b.sort === "Int" || b.sort === "Real");
    if (intBindings.length >= 2) {
      let foundDup: { filePath: string; line: number; values: string[] } | null = null;
      for (const step of path.steps) {
        const loc = resolveStepLocation(db, step.nodeId);
        if (!loc) continue;
        const lines = readSource(loc.filePath);
        if (!lines) continue;
        const idx = loc.line - 1;
        if (idx < 0 || idx >= lines.length) continue;
        const winStart = Math.max(0, idx - 2);
        const winEnd = Math.min(lines.length, idx + 3);
        const windowText = lines.slice(winStart, winEnd).join("\n");
        const dup = findLiteralArrayDuplicate(windowText);
        if (dup) {
          foundDup = { filePath: loc.filePath, line: loc.line, values: dup };
          break;
        }
      }
      if (foundDup) {
        // Pin the first two Int bindings to be equal (any duplicate
        // pair satisfies the canonical "(not (distinct ...))" shape).
        // We use the first two bindings as a stand-in for the pair
        // identified at the source level — v1 doesn't try to map
        // specific m_i constants to specific array indices.
        const [b1, b2] = intBindings;
        state.pathAssertions.push(
          `; kind-aware (set_uniqueness): literal-array duplicate at ${foundDup.filePath}:${foundDup.line} (${foundDup.values.join(",")}) — pinning ${b1.smt_constant} = ${b2.smt_constant}`,
        );
        state.pathAssertions.push(
          `(assert (= ${b1.smt_constant} ${b2.smt_constant}))`,
        );
        state.emittedPathConstraints++;
        state.realPathConstraints++;
      }
    }
  }

  return state;
}

/**
 * Map a Bool cardinality binding's constant name to a polarity:
 *   "ran"   = true means "the operation ran"
 *   "never" = true means "the operation did NOT run"
 * Returns null when the name is too ambiguous to read confidently.
 *
 * Heuristic, sanctioned for v1. Inverted polarity surfaces as a false
 * negative (undecidable), never as a false positive — we err toward
 * unpinned when ambiguous.
 */
function inferCardinalityPolarity(constantName: string): "ran" | "never" | null {
  const n = constantName.toLowerCase();
  if (/never|_zero|not_fired|did_not|no_run/.test(n)) return "never";
  if (/at_least_once|_ran|_fired|exactly_once|was_called|did_run/.test(n)) return "ran";
  return null;
}

/**
 * Scan a window of source text for a literal array containing a
 * duplicate string-literal element. Returns the duplicate value(s) or
 * null. Conservative — only fires on simple literal arrays of strings:
 *   `["GET", "POST", "GET"]`  →  ["GET"]
 *   `["a", "b", "a", "c"]`    →  ["a"]
 *
 * Misses: numeric duplicates, computed entries, multi-line arrays
 * straddling the window. Sanctioned for v1.
 */
function findLiteralArrayDuplicate(text: string): string[] | null {
  // Match an array literal containing only string-quoted entries.
  // Tolerates whitespace; bails on anything not matching the simple shape.
  const arrRe = /\[\s*((?:["'][^"']*["']\s*,\s*)+["'][^"']*["'])\s*,?\s*\]/g;
  let m: RegExpExecArray | null;
  while ((m = arrRe.exec(text)) !== null) {
    const inner = m[1];
    const items = inner.match(/["']([^"']*)["']/g);
    if (!items) continue;
    const values = items.map((s) => s.slice(1, -1));
    const seen = new Set<string>();
    const dups: string[] = [];
    for (const v of values) {
      if (seen.has(v)) dups.push(v);
      seen.add(v);
    }
    if (dups.length > 0) return dups;
  }
  return null;
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
