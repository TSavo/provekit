/**
 * Centralized model-tier selection for the fix loop.
 *
 * Each LLM-calling stage looks up its model tier via getModelTier(stageName).
 *
 * Tier philosophy (post Bug-1 v5 calibration):
 *
 *   - sonnet: the default. Reliable instruction-following, including the
 *             tool-use contract that requestStructuredJson depends on
 *             ("write JSON to {path}, do not return inline"). Cost
 *             difference vs haiku is pennies per call; reliability
 *             difference is whole runs aborting on tool-use violations.
 *   - opus: load-bearing reasoning where invariant correctness or
 *           substrate design rides on the output.
 *   - haiku: NOT used by default. Reserve for explicit cost-conscious
 *           runs via PROVEKIT_MODEL_OVERRIDE=haiku, accepting the
 *           reliability trade-off.
 *
 * Why no haiku in the default mapping:
 *
 * Bug-1 v5 aborted at Classify because haiku ignored the "use the Write
 * tool" instruction and dumped JSON inline as text. The structuredOutput
 * helper expected the file at the scratch path; missing file → throw →
 * whole loop aborts (~6 minutes of compute wasted). This was the SECOND
 * occurrence of haiku skipping the Write contract in our staging.
 *
 * Cost math: a classify call is ~3 KB in, ~1 KB out. Haiku ~$0.005,
 * sonnet ~$0.05. Per-call delta ~5 cents. A failed run wastes 6 minutes
 * + the LLM tokens already spent on intake + the user's time. The
 * sonnet upgrade is the right trade-off without ambiguity.
 *
 * Override modes (in priority order):
 *
 *   1. Per-stage env var:   PROVEKIT_MODEL_<STAGE>=opus
 *      Example:             PROVEKIT_MODEL_C1=sonnet
 *      Stage names are upper-cased and stripped of non-alnum chars before
 *      lookup, so "intake-report" maps to PROVEKIT_MODEL_INTAKEREPORT.
 *
 *   2. Global override:     PROVEKIT_MODEL_OVERRIDE=opus
 *      Forces every stage to the named tier. Useful for debugging when you
 *      want apples-to-apples comparison across the loop, OR for
 *      cost-conscious runs (PROVEKIT_MODEL_OVERRIDE=haiku) accepting the
 *      reliability trade-off.
 *
 *   3. Default tier from MODEL_TIERS below.
 *
 * Adding a new stage:
 *   - Default to sonnet unless the call's reasoning load is clearly
 *     load-bearing (invariant correctness, substrate design) — then opus.
 *   - Don't pick haiku for any stage that uses the Write tool or any
 *     other structured-output contract.
 */

export type ModelTier = "haiku" | "sonnet" | "opus";

/**
 * Stage-to-tier mapping. Keys are the literal stage strings used at call sites
 * (passed to requestStructuredJson({ stage }) or as model: arg to runAgentInOverlay).
 *
 * If a stage isn't listed here, getModelTier falls back to "sonnet" (the
 * defensive middle tier).
 */
export const MODEL_TIERS: Record<string, ModelTier> = {
  // -------------------------------------------------------------------------
  // B1: intake. Short-text parsing, structured-extraction prompts.
  // Was haiku in earlier versions for "speed"; upgraded to sonnet after
  // Bug-1 v5 showed haiku occasionally ignoring the Write-tool contract,
  // which aborts whole runs. Sonnet is reliable on tool use and the cost
  // delta is pennies per call.
  // -------------------------------------------------------------------------
  "intake-report": "sonnet",
  "intake-testFailure": "sonnet",
  "intake-gapReport": "sonnet",
  "intake-runtimeLog": "sonnet",

  // -------------------------------------------------------------------------
  // B2: classify. Short categorization. Sonnet for tool-use reliability.
  // -------------------------------------------------------------------------
  classify: "sonnet",

  // -------------------------------------------------------------------------
  // C1: formulateInvariant. Load-bearing; whole correctness story rides on
  // the SMT formula being right. Stays opus.
  // -------------------------------------------------------------------------
  C1: "opus",
  // C1.5 fidelity verifiers: keep the existing adaptive behaviour. The
  // adversaryModel() helper in invariantFidelity.ts derives the cross-LLM
  // model from the proposer's tier (opus → sonnet, sonnet/haiku → opus) so
  // that the cross-LLM diversity property is preserved. Listing sonnet here
  // is informational only; invariantFidelity.ts ignores this entry.
  "C1.5-crossLLM": "sonnet",
  "C1.5-traceability": "sonnet",
  "C1.5-proseOverlap": "sonnet",

  // -------------------------------------------------------------------------
  // C3: generateFixCandidate. Code edits in overlay; sonnet is the
  // sweet spot for tool-use code generation.
  // -------------------------------------------------------------------------
  "C3-candidateGen": "sonnet",
  "C3-agent": "sonnet",

  // -------------------------------------------------------------------------
  // C4: complementary. Adjacent-site fixes; same task class as C3.
  // -------------------------------------------------------------------------
  "C4-complementary": "sonnet",
  "C4-agent": "sonnet",

  // -------------------------------------------------------------------------
  // C5: regression test generation. Code synthesis for a small test file.
  // -------------------------------------------------------------------------
  "C5-testGen": "sonnet",

  // -------------------------------------------------------------------------
  // C6: principle proposal (sonnet) and capability spec (opus).
  // Capability proposal is a substrate-extension event: schema migrations +
  // extractor + DSL all at once. That's a load-bearing structural design
  // decision; opus.
  // -------------------------------------------------------------------------
  "C6-principleProposal": "sonnet",
  "C6-adversarial": "sonnet",
  "C6-capabilitySpec": "opus",
  "C6-capabilityAgent": "opus",
};

/**
 * Resolve the model tier for a stage name, honouring env overrides.
 *
 * Lookup order:
 *   1. PROVEKIT_MODEL_OVERRIDE (global)
 *   2. PROVEKIT_MODEL_<NORMALIZED_STAGE>
 *   3. MODEL_TIERS[stage]
 *   4. "sonnet" (defensive default)
 *
 * Invalid env values (anything not matching opus/sonnet/haiku) are ignored
 * with a single console warn, and lookup proceeds to the next step.
 */
export function getModelTier(stage: string): ModelTier {
  const globalOverride = readTierEnv("PROVEKIT_MODEL_OVERRIDE");
  if (globalOverride) return globalOverride;

  const normalized = normalizeStageEnv(stage);
  const stageOverride = readTierEnv(`PROVEKIT_MODEL_${normalized}`);
  if (stageOverride) return stageOverride;

  const mapped = MODEL_TIERS[stage];
  if (mapped) return mapped;

  return "sonnet";
}

/**
 * Validate an env var value and return it as a ModelTier, or null.
 * Logs a warning once per process per invalid var name.
 */
const warnedEnvVars = new Set<string>();
function readTierEnv(varName: string): ModelTier | null {
  const raw = process.env[varName];
  if (!raw) return null;
  const value = raw.trim().toLowerCase();
  if (value === "opus" || value === "sonnet" || value === "haiku") {
    return value;
  }
  if (!warnedEnvVars.has(varName)) {
    warnedEnvVars.add(varName);
    console.warn(
      `[modelTiers] ${varName}="${raw}" is not a valid tier (opus|sonnet|haiku); ignoring.`,
    );
  }
  return null;
}

/**
 * Normalize a stage string into an env-var-friendly suffix.
 * "intake-report"   → "INTAKEREPORT"
 * "C1.5-crossLLM"   → "C15CROSSLLM"
 * "C3-candidateGen" → "C3CANDIDATEGEN"
 */
function normalizeStageEnv(stage: string): string {
  return stage.replace(/[^a-zA-Z0-9]/g, "").toUpperCase();
}
