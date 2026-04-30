/**
 * Formulate-via-lifter — the architecture-correct invariant-formulation
 * Stage. Routes the LLM through the Appendix-C "one-size-fits-all"
 * template, captures TS-IR-language SURFACE text, lifts it through the
 * v2 lifter (`src/ir/lift/`), canonicalizes, and emits a propertyHash.
 *
 * Spec:
 *   docs/specs/2026-04-29-ts-ir-language.md §2 (two-LLM-call architecture)
 *   docs/specs/2026-04-29-ts-ir-language.md §9 (lifter dispatch)
 *   docs/specs/2026-04-29-ts-ir-language.md §15 (three-step unit of work)
 *   docs/specs/2026-04-29-ts-ir-language.md Appendix C (LLM template)
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

An invariant is a statement that must be true for all inputs the function
receives. You write them as predicate functions. Use \`must("name", predicate)\`
to declare each one. Group related invariants with \`describe()\`.

Use these functions, imported from 'provekit/ir/symbolic':

  describe(name, body)            // group invariants
  must(name, predicate)           // declare an invariant

  // Quantifiers — universal/existential claims over a sort
  forAll(sort, x => predicate)    // "for all x of sort, predicate is true"
  exists(sort, x => predicate)    // "there exists x of sort where predicate is true"

  // Connectives
  implies(a, b)                   // a => b
  and(a, b), or(a, b), not(a), iff(a, b)

  // Numbers
  num(n)                          // an integer constant
  real(n)                         // a real constant
  str(s)                          // a string constant
  bool(b)                         // a boolean constant
  add(a, b), sub(a, b), mul(a, b), div(a, b), neg(a)

  // Comparisons
  eq(a, b)                        // a === b
  neq(a, b)                       // a !== b
  lt(a, b), lte(a, b), gt(a, b), gte(a, b)

  // Built-ins (use these instead of native parseInt, Math.abs, etc.)
  parseInt(s), parseFloat(s)
  isNaN(n), isFinite(n), isInteger(n)
  abs(n), max(a, b), min(a, b), floor(n), ceil(n), sqrt(n), sign(n)
  stringLength(s), stringIncludes(s, sub)
  arrayLength(a), arrayIncludes(a, item)

  // Sorts
  Int, Real, Bool, String as StringSort

== HOW TO WRITE AN INVARIANT ==

import {
  describe, must, forAll, exists, eq, gt, gte, parseInt, num, str, abs, Int,
  String as StringSort,
} from 'provekit/ir/symbolic';

describe("parseInt", () => {
  must("zero string parses to zero",
    eq(parseInt(str("0")), num(0))
  );

  must("preserves non-negative integers",
    forAll(Int, (n) =>
      gte(n, num(0))   // for all n where n >= 0
        ? eq(parseInt(/* toString(n) */), n)
        : forAll(Int, (k) => gt(num(1), num(0)))  // (placeholder, no toString primitive yet)
    )
  );
});

describe("Math.abs", () => {
  must("never returns negative",
    forAll(Int, (x) => gte(abs(x), num(0)))
  );
});

== RULES ==

- Use \`must\`, not \`it\` (invariants are obligations, not test observations).
- Use \`eq\`, \`gt\`, \`add\`, etc. — never the native ===, >, +, *, etc.
- Use \`parseInt\`, \`abs\`, etc. from the import — never \`global.parseInt\` or \`Math.abs\`.
- No async, no loops, no try/catch, no this/new, no mutations.

== DIFF ==
{{diff}}

== TESTS ==
{{tests}}

== INTENT ==
{{intent_text}}

== TARGET FILE ==
{{file_paths_for_invariant_files}}

Output: TypeScript source for a \`.invariant.ts\` file. Nothing else.
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
