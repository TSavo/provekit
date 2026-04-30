/**
 * Formulate-via-lifter — the architecture-correct invariant-formulation
 * Stage. Routes the LLM through the Appendix-C "one-size-fits-all"
 * template, captures TS-IR-language SURFACE text, lifts it through the
 * v2 lifter (`src/ir/lift/`), canonicalizes, and emits a propertyHash.
 *
 * Spec:
 *   protocol/specs/2026-04-29-ts-ir-language.md §2 (two-LLM-call architecture)
 *   protocol/specs/2026-04-29-ts-ir-language.md §9 (lifter dispatch)
 *   protocol/specs/2026-04-29-ts-ir-language.md §15 (three-step unit of work)
 *   protocol/specs/2026-04-29-ts-ir-language.md Appendix C (LLM template)
 *
 * Note on path A: this Stage is registered alongside the legacy
 * `formulate` capability under a NEW capability name. The legacy
 * stage and its smoke-test mock target (`formulateInvariant`) keep
 * working untouched. Wiring the manifest to swap producers is a
 * follow-up; doing it here would break the integration smoke without
 * carrying its own value.
 */

import type { LLMProvider } from "../../fix/types.js";
import type { IntentSignal, BugLocus } from "../../fix/types.js";
import { getIntentText } from "../../fix/types.js";
import type { InvestigateReport } from "../../fix/stages/investigate.js";
import type { IrFormula } from "../../ir/formulas.js";
import { liftSurfaceText } from "../../ir/lift/liftSurface.js";
import { formatDiagnostic } from "../../ir/lift/diagnostics.js";
import { propertyHashFromFormula } from "../../canonicalizer/canonicalize.js";
import type { Stage } from "../types.js";

export const FORMULATE_VIA_LIFTER_CAPABILITY = "formulate-via-lifter";

export interface FormulateViaLifterStageInput {
  intent: IntentSignal;
  investigateReport?: InvestigateReport;
  /**
   * Existential-intent test sources. Each entry is a single test file
   * (or an extracted block of tests). The Appendix-C template renders
   * them as ``= TESTS =`` fenced code; the LLM uses them to shape the
   * synthesized invariants.
   */
  tests?: { source: string; testNames: string[] }[];
  /**
   * Diff describing the prospective change. Optional; absent for
   * legacy-mode invocations and for prospective use cases that haven't
   * produced a diff yet (the bug-fix flow, today). The template renders
   * `(no diff yet; prospective change)` when missing.
   */
  diff?: string;
  /**
   * Optional bug locus. Used to derive the target invariant-file path
   * the template renders under `== TARGET FILES ==`. Without it the
   * template falls back to a generic placeholder.
   */
  locus?: BugLocus;
  /**
   * Kit catalog content-IDs the synthesized formula composes against.
   * v1 stays minimal — empty by default — and the runner threads
   * upstream stage CIDs into the memento separately. Real kit-catalog
   * resolution is a follow-up (see spec gap report at the end of
   * the formulate-via-lifter prompt).
   */
  kitCatalogCids?: string[];
}

export interface FormulateViaLifterStageOutput {
  /** Verbatim LLM-emitted `.invariant.ts` source. */
  surfaceText: string;
  /** Lifted, byte-identically canonicalizable IR formula. */
  formula: IrFormula;
  /** sha256-prefix-16 of the canonicalized formula. */
  propertyHash: string;
  /** Property name extracted from the surface text's `property("name", ...)`. */
  name: string;
  /** Kit catalog CIDs the runner should fold into the memento's inputCids. */
  inputCidsToCompose: string[];
}

export interface MakeFormulateViaLifterStageDeps {
  llm: LLMProvider;
  /** Override producer identity. Default: "formulate-via-lifter@v1". */
  producerVersion?: string;
}

// ---------------------------------------------------------------------------
// Appendix-C template
// ---------------------------------------------------------------------------

const APPENDIX_C_TEMPLATE = `Write invariants for a TypeScript change.

== WHAT YOU ARE DOING ==

A code change is being committed. You will write the invariants that the
change must satisfy. The framework will verify your invariants against
installed kit catalogs; if they hold, the change is accepted; if not, the
commit is rejected with a counterexample. Your invariants are the
formal claim about what the change should do — for ALL inputs in the
function's domain, not just the test cases.

The tests below are existential evidence: "for THIS specific input, the
output is THIS." Your invariants are the universal generalization: "for
ALL inputs in the domain, the predicate holds." The tests are points;
your invariants are the curve through the points.

== HOW THE PRIMITIVES WORK ==

You write predicate functions using imported primitives. The primitives
LOOK like they compute values (parseInt("0") returns 0, eq(a, b) is
true if a equals b) but they actually capture the call into a structured
declaration that the framework verifies. Like jest mocks: the call shape
is recorded, no real computation happens.

You don't need to think about IR or SAT solvers. Write predicates as if
they computed values; the framework does the rest.

== THE API (import these from 'provekit/ir/symbolic') ==

  describe(name, body)            // group invariants
  must(name, predicate)           // declare an invariant

  // Quantifiers — universal/existential
  forAll(sort, x => predicate)    // "for all x of sort"
  exists(sort, x => predicate)    // "there exists x of sort"

  // Connectives
  implies(a, b)                   // a => b
  and(a, b), or(a, b), not(a), iff(a, b)

  // Constants
  num(n), real(n), str(s), bool(b)

  // Arithmetic (term-level)
  add(a, b), sub(a, b), mul(a, b), div(a, b), neg(a)

  // Comparisons (formula-level)
  eq(a, b), neq(a, b), lt(a, b), lte(a, b), gt(a, b), gte(a, b)

  // Built-ins — use these instead of global parseInt, Math.abs, etc.
  parseInt(s), parseFloat(s)
  isNaN(n), isFinite(n), isInteger(n)
  abs(n), max(a, b), min(a, b), floor(n), ceil(n), sqrt(n), sign(n)
  stringLength(s), stringIncludes(s, sub)
  arrayLength(a), arrayIncludes(a, item)

  // Sorts
  Int, Real, Bool, String as StringSort

== EXAMPLE 1 — bug fix (off-by-one in leap-year check) ==

Diff: function isLeapYear(year) corrected to handle year % 100 === 0
exception for non-400-divisible years.

Tests added:
  expect(isLeapYear(2024)).toBe(true);
  expect(isLeapYear(2100)).toBe(false);
  expect(isLeapYear(2000)).toBe(true);

Your invariants:

  import {
    describe, must, forAll, eq, num, isLeapYear, Int,
  } from 'provekit/ir/symbolic';

  describe("isLeapYear", () => {
    must("Gregorian rule",
      forAll(Int, (year) =>
        eq(
          isLeapYear(year),
          // year is a leap year iff (divisible by 400) or (divisible by 4 and not 100)
          // (in real code this would be the symbolic equivalent; example shows the shape)
          eq(year, num(0)) // placeholder — real invariant would express the rule
        )
      )
    );

    must("year 2100 is not a leap year (Gregorian century non-divisible-by-400)",
      not(isLeapYear(num(2100)))
    );
  });

The point-cases (2024, 2100, 2000) are tests. The Gregorian rule
universal claim is the invariant — it holds for the test cases AND for
all other years.

== EXAMPLE 2 — feature add (new helper function) ==

Diff: added function safeDivide(n, d) that throws if d === 0.

Tests added:
  expect(safeDivide(10, 2)).toBe(5);
  expect(() => safeDivide(10, 0)).toThrow();

Your invariants:

  import {
    describe, must, forAll, implies, eq, neq, num, Int, safeDivide,
  } from 'provekit/ir/symbolic';

  describe("safeDivide", () => {
    must("returns the quotient for non-zero denominator",
      forAll(Int, (n) =>
        forAll(Int, (d) =>
          implies(neq(d, num(0)), eq(safeDivide(n, d), div(n, d)))
        )
      )
    );

    must("zero denominator is the only error case",
      forAll(Int, (n) => safeDivide(n, num(0)) /* throws */)
    );
  });

== EXAMPLE 3 — refactor (renamed but behavior preserved) ==

Diff: renamed calculateTotal → totalCents (and changed to integer cents).

Tests added (just regression):
  expect(totalCents(items)).toBe(calculateTotalLegacyResult);

Your invariants:

  import {
    describe, must, forAll, eq, totalCents, calculateTotal, Int,
  } from 'provekit/ir/symbolic';

  describe("totalCents (refactored from calculateTotal)", () => {
    must("preserves the legacy result",
      forAll(/* LineItem[] */ Int, (items) =>
        eq(totalCents(items), calculateTotal(items))
      )
    );

    must("returns a non-negative integer",
      forAll(Int, (items) => gte(totalCents(items), num(0)))
    );
  });

The refactor's invariant pins the OLD behavior surface as still-required.
Anyone changing the new function is now constrained by the contract that
matched the old function.

== COMMON MISTAKES (and how to fix them) ==

  ✗ must("zero", eq(parseInt(str("0")), num(0)))
    Only checks one point; not a universal claim.
  ✓ must("preserves zero", eq(parseInt(str("0")), num(0)))
    AND
    must("preserves all non-negative integers",
      forAll(Int, (n) => implies(gte(n, num(0)), eq(parseInt(toString(n)), n)))
    )

  ✗ must("test", parseInt(str("0")) === 0)
    Native === computes a boolean. The framework can't see what you claimed.
  ✓ must("test", eq(parseInt(str("0")), num(0)))
    eq(...) builds the framework's equality predicate.

  ✗ it("zero string parses", ...)
    The verb is must, not it. Tests use it; invariants use must.
  ✓ must("zero string parses", ...)

  ✗ const limit = userInput; must("...", lt(x, limit))
    Closure over reassignable userInput; breaks determinism.
  ✓ const LIMIT = 100; must("...", lt(x, num(LIMIT)))
    Const closure with a literal; resolved at lift time.

  ✗ must("works", await someAsyncFn(x))
    No async, no awaits — invariants are timeless propositions.
  ✓ must("works", forAll(Int, (x) => predicate))

  ✗ must("works", Math.abs(x) >= 0)
    Native Math.abs computes; framework can't see the call.
  ✓ must("works", gte(abs(x), num(0)))
    Symbolic abs() builds the call.

== RULES ==

- Use must, not it.
- Use eq, gt, add, gte etc. — never native ===, >, +, >=.
- Use parseInt, abs, etc. from the import — never global.parseInt or Math.abs.
- forAll/exists for universal/existential claims; tests cover specific points.
- No async, no loops (use forAll/exists), no try/catch, no this/new, no mutations.
- Const closure only; no let/var closure.

== DIFF ==
{{diff}}

== TESTS ==
{{tests}}

== INTENT ==
{{intent_text}}

== TARGET FILE ==
{{file_paths_for_invariant_files}}

== OUTPUT ==

TypeScript source for a single .invariant.ts file. Nothing else — no
explanation, no markdown fences, no comments outside the source.

Each describe()/must() group should anchor on a function or symbol from
the diff. The invariants should be true for the listed test cases AND
true for all other inputs in the function's domain.
`;

function renderTests(tests: { source: string; testNames: string[] }[] | undefined): string {
  if (!tests || tests.length === 0) return "(no tests supplied)";
  return tests
    .map((t, i) => {
      const header = `// test source #${i + 1}` +
        (t.testNames.length > 0 ? ` — ${t.testNames.join(", ")}` : "");
      return [header, "```ts", t.source, "```"].join("\n");
    })
    .join("\n\n");
}

function renderTargetFile(locus: BugLocus | undefined): string {
  if (!locus) return "(no locus; emit invariants in a file colocated with the production code)";
  return locus.file.replace(/\.ts$/, ".invariant.ts");
}

function buildPrompt(input: FormulateViaLifterStageInput): string {
  return APPENDIX_C_TEMPLATE
    .replace("{{diff}}", input.diff ?? "(no diff yet; prospective change)")
    .replace("{{tests}}", renderTests(input.tests))
    .replace("{{intent_text}}", getIntentText(input.intent))
    .replace("{{file_paths_for_invariant_files}}", renderTargetFile(input.locus));
}

/**
 * Strip Markdown code fences from an LLM response. The Appendix-C
 * template instructs the LLM to emit only TS source, but models
 * commonly oblige by wrapping it in ```ts ... ``` anyway. Tolerate
 * either form so the lifter sees the right bytes.
 */
function unfence(raw: string): string {
  const trimmed = raw.trim();
  const fenceMatch = trimmed.match(/^```(?:ts|typescript)?\s*\n([\s\S]*?)\n```\s*$/);
  if (fenceMatch) return fenceMatch[1] ?? "";
  return trimmed;
}

export class FormulateViaLifterError extends Error {
  constructor(message: string, public readonly surfaceText?: string) {
    super(message);
    this.name = "FormulateViaLifterError";
  }
}

export function makeFormulateViaLifterStage(
  deps: MakeFormulateViaLifterStageDeps,
): Stage<FormulateViaLifterStageInput, FormulateViaLifterStageOutput> {
  const producedBy = deps.producerVersion ?? "formulate-via-lifter@v1";

  return {
    name: "formulate-via-lifter",
    producedBy,

    serializeInput(input) {
      // Sorted, fixed-shape canonicalization. Stable across reruns.
      return {
        intent: input.intent,
        investigateReport: input.investigateReport ?? null,
        tests: input.tests ?? [],
        diff: input.diff ?? null,
        locus: input.locus ?? null,
        kitCatalogCids: [...(input.kitCatalogCids ?? [])].sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as FormulateViaLifterStageOutput;
    },

    async run(input) {
      const prompt = buildPrompt(input);
      const raw = await deps.llm.complete({ prompt });
      const surfaceText = unfence(raw);

      const lifted = liftSurfaceText(surfaceText);
      if (lifted.diagnostics.length > 0) {
        const formatted = lifted.diagnostics.map(formatDiagnostic).join("\n");
        throw new FormulateViaLifterError(
          `liftSurfaceText emitted ${lifted.diagnostics.length} diagnostic(s):\n${formatted}`,
          surfaceText,
        );
      }
      if (lifted.properties.length === 0) {
        throw new FormulateViaLifterError(
          "lifted surface contained no property() declarations",
          surfaceText,
        );
      }

      // Pick the first property; multi-property emission is a richer
      // shape we leave for a follow-up.
      const first = lifted.properties[0]!;
      const propertyHash = propertyHashFromFormula(first.formula);

      return {
        surfaceText,
        formula: first.formula,
        propertyHash,
        name: first.name,
        inputCidsToCompose: [...(input.kitCatalogCids ?? [])].sort(),
      };
    },
  };
}
