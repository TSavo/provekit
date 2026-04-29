/**
 * B3: classify — given an IntentSignal + BugLocus, determine what kind of
 * remediation is needed and produce a RemediationPlan.
 *
 * The classifier prompt is built dynamically from the remediation layer
 * registry. Adding a new layer = register it and its description + examples
 * appear in the next classify() call's prompt. No code change required.
 */

import "./remediationLayers/index.js";
import {
  listRemediationLayers,
  getRemediationLayer,
} from "./remediationLayerRegistry.js";
import { requestStructuredJson } from "./llm/structuredOutput.js";
import { getPromptStore } from "../llm/promptStore.js";

// ---------------------------------------------------------------------------
// Classify prompt artifact: classify.prompt
//
// One bp namespace. Four runtime placeholders. Same pattern as the C-stages.
//
// Future evolution warning: bp.evolve on classify.prompt MUST preserve
// {{LAYER_LIST}}, {{SUMMARY}}, {{INTENT_TEXT}}, {{LOCUS}} placeholders verbatim.
// ---------------------------------------------------------------------------

const CLASSIFY_PROMPT_TEMPLATE = `You are classifying an intent into a remediation layer. The intent may be a bug report (a failure that should not happen), a change request (a property that should hold), or a property assertion (the property stated directly). Treat them uniformly: the intent describes a property the code should satisfy.

Here are the available layers:

{{LAYER_LIST}}

Given this intent:
  Summary: {{SUMMARY}}
  User text: {{INTENT_TEXT}}
  Code location: {{LOCUS}}

Produce JSON with exactly these keys:
  primaryLayer: string (one of the layer names above)
  secondaryLayers: string[] (zero or more additional layer names)
  artifacts: Array<{kind: string, rationale?: string, envVar?: string, site?: string, bugClassName?: string}>
  rationale: string (why you chose this layer)

Respond with JSON only. No prose before or after.`;

const CLASSIFY_PROMPT_DISCRIMINATOR = "2026-04-29";
import { getModelTier } from "./modelTiers.js";
import type {
  RemediationLayerDescriptor,
} from "./remediationLayerRegistry.js";
import {
  type IntentSignal,
  type BugLocus,
  type RemediationPlan,
  type PlannedArtifact,
  type LLMProvider,
  getIntentText,
} from "./types.js";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

export class ClassifyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ClassifyError";
  }
}

// ---------------------------------------------------------------------------
// Prompt building (exported for tests)
// ---------------------------------------------------------------------------

/**
 * Build the classifier prompt from the registered layers and the signal/locus.
 * Exported so tests can inspect the prompt without making a real LLM call.
 */
export async function buildPrompt(
  signal: IntentSignal,
  locus: BugLocus | null,
  layers: readonly RemediationLayerDescriptor[],
  projectRoot?: string,
): Promise<{ prompt: string; revisions: Array<{ key: string; revisionId: string }> }> {
  const layerList = layers
    .map(
      (l, i) =>
        `${i + 1}. ${l.name} — ${l.description}\n   ${l.promptHint}`,
    )
    .join("\n\n");

  const locusStr = locus
    ? `${locus.file}:${locus.line}${locus.function ? ` (${locus.function})` : ""}`
    : "not resolved";

  const revisions: Array<{ key: string; revisionId: string }> = [];
  let templateBody = CLASSIFY_PROMPT_TEMPLATE;
  if (projectRoot) {
    const rev = await getPromptStore(projectRoot).get(
      "classify.prompt",
      CLASSIFY_PROMPT_TEMPLATE,
      CLASSIFY_PROMPT_DISCRIMINATOR,
    );
    templateBody = rev.body;
    revisions.push({ key: "classify.prompt", revisionId: rev.id });
  }

  const renderVars: Record<string, string> = {
    LAYER_LIST: layerList,
    SUMMARY: signal.summary,
    INTENT_TEXT: getIntentText(signal),
    LOCUS: locusStr,
  };
  let prompt = templateBody;
  for (const [k, v] of Object.entries(renderVars)) {
    prompt = prompt.replaceAll(`{{${k}}}`, v);
  }
  return { prompt, revisions };
}

// ---------------------------------------------------------------------------
// JSON schema for LLM response validation
// ---------------------------------------------------------------------------

const CLASSIFY_SCHEMA = {
  type: "object",
  required: ["primaryLayer", "secondaryLayers", "artifacts", "rationale"],
  properties: {
    primaryLayer: { type: "string" },
    secondaryLayers: { type: "array", items: { type: "string" } },
    artifacts: {
      type: "array",
      items: {
        type: "object",
        required: ["kind"],
        properties: {
          kind: { type: "string" },
          rationale: { type: "string" },
          envVar: { type: "string" },
          site: { type: "string" },
          bugClassName: { type: "string" },
        },
      },
    },
    rationale: { type: "string" },
  },
};

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

interface ParsedResponse {
  primaryLayer: string;
  secondaryLayers: string[];
  artifacts: PlannedArtifact[];
  rationale: string;
}

function validateClassifyResponse(parsed: unknown): ParsedResponse {
  if (typeof parsed !== "object" || parsed === null) {
    throw new ClassifyError(`classify: LLM response is not an object`);
  }

  const obj = parsed as Record<string, unknown>;

  if (typeof obj["primaryLayer"] !== "string") {
    throw new ClassifyError(`classify: LLM response missing string primaryLayer`);
  }
  if (!Array.isArray(obj["secondaryLayers"])) {
    throw new ClassifyError(`classify: LLM response missing array secondaryLayers`);
  }
  if (!Array.isArray(obj["artifacts"])) {
    throw new ClassifyError(`classify: LLM response missing array artifacts`);
  }
  if (typeof obj["rationale"] !== "string") {
    throw new ClassifyError(`classify: LLM response missing string rationale`);
  }

  const artifacts: PlannedArtifact[] = (obj["artifacts"] as unknown[]).map(
    (a) => {
      if (typeof a !== "object" || a === null) {
        throw new ClassifyError(`classify: artifact is not an object`);
      }
      const artifact = a as Record<string, unknown>;
      if (typeof artifact["kind"] !== "string") {
        throw new ClassifyError(`classify: artifact missing string kind`);
      }
      const planned: PlannedArtifact = { kind: artifact["kind"] as string };
      if (typeof artifact["rationale"] === "string") planned.rationale = artifact["rationale"];
      if (typeof artifact["envVar"] === "string") planned.envVar = artifact["envVar"];
      if (typeof artifact["site"] === "string") planned.site = artifact["site"];
      if (typeof artifact["bugClassName"] === "string") planned.bugClassName = artifact["bugClassName"];
      return planned;
    },
  );

  return {
    primaryLayer: obj["primaryLayer"] as string,
    secondaryLayers: (obj["secondaryLayers"] as unknown[]).map((s) => {
      if (typeof s !== "string") {
        throw new ClassifyError(`classify: secondaryLayer entry is not a string`);
      }
      return s;
    }),
    artifacts,
    rationale: obj["rationale"] as string,
  };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Classify a IntentSignal into a RemediationPlan.
 *
 * The classifier prompt is built dynamically from the registered remediation
 * layers — adding a layer via registerRemediationLayer() changes what the LLM
 * sees on the next call with no code changes here.
 */
export async function classify(
  signal: IntentSignal,
  locus: BugLocus | null,
  llm: LLMProvider,
  projectRoot?: string,
): Promise<RemediationPlan> {
  // 1. Build the classifier prompt dynamically from the registered layers.
  const layers = listRemediationLayers();
  const { prompt } = await buildPrompt(signal, locus, layers, projectRoot);

  // 2. LLM call, schema-validated JSON response.
  //    requestStructuredJson handles JSON parsing + writes via Write tool in
  //    agent mode (see src/fix/llm/structuredOutput.ts). We catch and rewrap
  //    parse-layer errors as ClassifyError to preserve the existing API.
  let parsed: ParsedResponse;
  try {
    parsed = await requestStructuredJson<ParsedResponse>({
      prompt,
      llm,
      stage: "classify",
      model: getModelTier("classify"),
      schema: CLASSIFY_SCHEMA,
      schemaCheck: validateClassifyResponse,
    });
  } catch (e) {
    if (e instanceof ClassifyError) throw e;
    throw new ClassifyError(e instanceof Error ? e.message : String(e));
  }

  // 3. Validate primaryLayer against the registry
  if (!getRemediationLayer(parsed.primaryLayer)) {
    throw new ClassifyError(
      `LLM returned unknown primary layer '${parsed.primaryLayer}'. Registered: ${layers.map((l) => l.name).join(", ")}`,
    );
  }

  // 4. Validate each secondary layer
  for (const sec of parsed.secondaryLayers) {
    if (!getRemediationLayer(sec)) {
      throw new ClassifyError(
        `LLM returned unknown secondary layer '${sec}'. Registered: ${layers.map((l) => l.name).join(", ")}`,
      );
    }
  }

  return {
    signal,
    locus,
    primaryLayer: parsed.primaryLayer,
    secondaryLayers: parsed.secondaryLayers,
    artifacts: parsed.artifacts,
    rationale: parsed.rationale,
  };
}
