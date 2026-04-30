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

const APPENDIX_C_TEMPLATE = `You are writing invariants for a TypeScript codebase using the ProvekIt framework.

Below is a code diff. Below that is the test code added or modified in the same
diff. Below that is a description of the developer's intent for this change.

Your task: write invariants in TypeScript using the symbolic-primitives API.
Each must() call captures a property the modified code should satisfy. The
framework runs your output to emit the IR; running each primitive (parseInt,
abs, eq, gt, forAll, etc.) builds an IR data structure rather than computing
a value.

- Pass for all the listed tests (the tests are existential examples of intent)
- Be consistent with the diff's intended semantics
- Capture properties the modified function should satisfy for ALL inputs in the domain
- Use ONLY the symbolic primitives below; do not call native parseInt or Math.abs

Output: TypeScript source for a \`.invariant.ts\` file. Do not output anything else.

== API (use these, exactly these) ==

import {
  describe, must,                           // structure + invariant declarations
  forAll, exists, implies, iff, and, or, not, // quantifiers + connectives
  parseInt, parseFloat,                     // number-parsing primitives
  abs, max, min, floor, ceil, sqrt, sign,   // Math primitives
  isNaN, isFinite, isInteger,               // predicate primitives
  num, real, str, bool,                     // constants
  add, sub, mul, div, neg,                  // term arithmetic
  eq, neq, lt, lte, gt, gte,                // atomic predicates
  Int, Real, Bool, String as StringSort,    // sorts
} from 'provekit/ir/symbolic';

Each primitive returns an IR data structure. Running your file inside the
framework's collector produces the IR; no AST walking, no tsc Compiler API.

== USAGE SHAPE ==

describe("topic", () => {
  must("invariant-name",
    forAll(Int, (x) => gt(abs(x), num(-1)))
  );

  describe("nested topic", () => {
    must("nested-invariant",
      exists(StringSort, (s) => eq(parseInt(s), num(0)))
    );
  });
});

== FORBIDDEN ==

- Native operators (===, +, *, etc.) — use eq(), add(), mul() etc. instead
- async/await, generators, Promise
- for/while/do loops — use forAll / exists / .every / .some
- Mutations (=, ++, --, .push, etc.)
- try/catch/throw
- this, new, prototype access, classes
- Side-effecting calls (the symbolic primitives are pure IR builders)
- Closure over mutable bindings (let/var); only const closures (resolved at lift)

== DIFF ==
{{diff}}

== TESTS (existential intent) ==
{{tests}}

== INTENT (universal context) ==
{{intent_text}}

== TARGET FILES ==
{{file_paths_for_invariant_files}}

Output the .invariant.ts source. The verb is must, not it: invariants are
obligations, not observations. Wrap related invariants in describe blocks.
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
