/**
 * Centralized model-tier selection for the fix loop.
 *
 * Each LLM-calling stage looks up its model tier via getModelTier(stageName).
 * The default mapping in MODEL_TIERS calibrates cost/latency:
 *
 *   - haiku: short-text parsing (intake adapters, classify) — opus is overkill.
 *   - sonnet: code generation + diffing (C3, C4, C5, C6 principle proposal)
 *             where we need quality but not the load-bearing reasoning of opus.
 *   - opus: load-bearing reasoning (C1 invariant, C6 capability spec).
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
 *      want apples-to-apples comparison across the loop.
 *
 *   3. Default tier from MODEL_TIERS below.
 *
 * Adding a new stage:
 *   - Pick a tier that matches the call's reasoning load.
 *   - Add an entry to MODEL_TIERS keyed by the stage string the call site
 *     passes to requestStructuredJson / runAgentInOverlay.
 *   - When in doubt, sonnet is the safe middle.
 */

export type ModelTier = "haiku" | "sonnet" | "opus";

/**
 * Stage-to-tier mapping. Keys are the literal stage strings used at call sites
 * (passed to requestStructuredJson({ stage }) or as model: arg to runAgentInOverlay).
 *
 * If a stage isn't listed here, getModelTier falls back to "sonnet" — the
 * defensive middle tier.
 */
export const MODEL_TIERS: Record<string, ModelTier> = {
  // -------------------------------------------------------------------------
  // B1: intake — short-text parsing, structured-extraction prompts.
  // Opus is overkill; haiku handles these in <1s.
  // -------------------------------------------------------------------------
  "intake-report": "haiku",
  "intake-testFailure": "haiku",
  "intake-gapReport": "haiku",
  "intake-runtimeLog": "haiku",

  // -------------------------------------------------------------------------
  // B2: classify — short categorization.
  // -------------------------------------------------------------------------
  classify: "haiku",

  // -------------------------------------------------------------------------
  // C1: formulateInvariant — load-bearing. Whole correctness story rides on
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
  // C3: generateFixCandidate — code edits in overlay. Sonnet is the
  // sweet spot for tool-use code generation.
  // -------------------------------------------------------------------------
  "C3-candidateGen": "sonnet",
  "C3-agent": "sonnet",

  // -------------------------------------------------------------------------
  // C4: complementary — adjacent-site fixes. Same task class as C3.
  // -------------------------------------------------------------------------
  "C4-complementary": "sonnet",
  "C4-agent": "sonnet",

  // -------------------------------------------------------------------------
  // C5: regression test generation — code synthesis for a small test file.
  // -------------------------------------------------------------------------
  "C5-testGen": "sonnet",

  // -------------------------------------------------------------------------
  // C6: principle proposal (sonnet) and capability spec (opus).
  // Capability proposal is a substrate-extension event: schema migrations +
  // extractor + DSL all at once. That's a load-bearing structural design
  // decision — opus.
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
