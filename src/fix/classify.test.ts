import { describe, it, expect, beforeEach } from "vitest";
import { classify, buildPrompt, ClassifyError } from "./classify.js";
import {
  _clearRemediationLayerRegistry,
  registerRemediationLayer,
  listRemediationLayers,
} from "./remediationLayerRegistry.js";
import { registerAll } from "./remediationLayers/index.js";
import type { BugSignal, BugLocus, LLMProvider } from "./types.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeSignal(overrides?: Partial<BugSignal>): BugSignal {
  return {
    source: "test",
    rawText: "test bug",
    summary: "function crashes on empty array",
    failureDescription: "divide by zero when array is empty",
    codeReferences: [],
    ...overrides,
  };
}

function makeLocus(overrides?: Partial<BugLocus>): BugLocus {
  return {
    file: "src/math.ts",
    line: 42,
    confidence: 1.0,
    primaryNode: "node-1",
    containingFunction: "node-0",
    relatedFunctions: [],
    dataFlowAncestors: [],
    dataFlowDescendants: [],
    dominanceRegion: [],
    postDominanceRegion: [],
    ...overrides,
  };
}

/** StubLLMProvider: returns canned JSON when the prompt contains a key substring. */
class StubLLMProvider implements LLMProvider {
  public lastPrompt: string = "";

  constructor(
    private readonly cannedResponse: string,
    private readonly matchKey?: string,
  ) {}

  async complete(params: { prompt: string; model?: string; schema?: object }): Promise<string> {
    this.lastPrompt = params.prompt;
    if (this.matchKey !== undefined && !params.prompt.includes(this.matchKey)) {
      throw new Error(
        `StubLLMProvider: prompt did not contain expected key "${this.matchKey}"`,
      );
    }
    return this.cannedResponse;
  }
}

function makeCodeInvariantResponse(): string {
  return JSON.stringify({
    primaryLayer: "code_invariant",
    secondaryLayers: [],
    artifacts: [
      { kind: "code_patch", rationale: "fix the divide by zero" },
      { kind: "regression_test", rationale: "verify empty array is handled" },
    ],
    rationale: "This is a logic error — divide by zero on empty array input.",
  });
}

function makeConfigResponse(): string {
  return JSON.stringify({
    primaryLayer: "config",
    secondaryLayers: [],
    artifacts: [
      { kind: "startup_assert", envVar: "GOOGLE_API_KEY", rationale: "assert key present at startup" },
      { kind: "documentation", rationale: "document required env vars" },
    ],
    rationale: "Missing environment variable causes the failure.",
  });
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("classify()", () => {
  beforeEach(() => {
    _clearRemediationLayerRegistry();
    registerAll();
  });

  // Test 1: code-invariant-shaped signal
  it("classifies code-invariant signal to primaryLayer: code_invariant", async () => {
    const stub = new StubLLMProvider(makeCodeInvariantResponse());
    const plan = await classify(makeSignal(), makeLocus(), stub);

    expect(plan.primaryLayer).toBe("code_invariant");
    expect(plan.secondaryLayers).toEqual([]);
    expect(plan.artifacts).toHaveLength(2);
    expect(plan.artifacts[0]!.kind).toBe("code_patch");
    expect(plan.artifacts[1]!.kind).toBe("regression_test");
    expect(plan.signal).toBeDefined();
    expect(plan.locus).toBeDefined();
  });

  // Test 2: config-shaped signal returns config + startup_assert artifact
  it("classifies config signal to primaryLayer: config with startup_assert artifact", async () => {
    const stub = new StubLLMProvider(makeConfigResponse());
    const plan = await classify(
      makeSignal({ summary: "missing GOOGLE_API_KEY", failureDescription: "env var not set" }),
      null,
      stub,
    );

    expect(plan.primaryLayer).toBe("config");
    expect(plan.artifacts.some((a) => a.kind === "startup_assert")).toBe(true);
    const assertArtifact = plan.artifacts.find((a) => a.kind === "startup_assert");
    expect(assertArtifact?.envVar).toBe("GOOGLE_API_KEY");
  });

  // Test 3: throws ClassifyError on unknown primary layer
  it("throws ClassifyError if LLM returns an unknown primary layer", async () => {
    const badResponse = JSON.stringify({
      primaryLayer: "unicorn_layer",
      secondaryLayers: [],
      artifacts: [],
      rationale: "made up",
    });
    const stub = new StubLLMProvider(badResponse);

    await expect(classify(makeSignal(), null, stub)).rejects.toThrow(ClassifyError);
    await expect(classify(makeSignal(), null, stub)).rejects.toThrow(
      "unknown primary layer 'unicorn_layer'",
    );
  });

  // Test 4: throws ClassifyError on unknown secondary layer
  it("throws ClassifyError if LLM returns an unknown secondary layer", async () => {
    const badResponse = JSON.stringify({
      primaryLayer: "code_invariant",
      secondaryLayers: ["phantom_layer"],
      artifacts: [],
      rationale: "with bad secondary",
    });
    const stub = new StubLLMProvider(badResponse);

    await expect(classify(makeSignal(), null, stub)).rejects.toThrow(ClassifyError);
    await expect(classify(makeSignal(), null, stub)).rejects.toThrow(
      "unknown secondary layer 'phantom_layer'",
    );
  });

  // Test 5: classifier prompt contains ALL 8 registered layers by name
  it("classifier prompt contains all 8 registered layer names", () => {
    const layers = listRemediationLayers();
    expect(layers).toHaveLength(8);

    const prompt = buildPrompt(makeSignal(), null, layers);
    const expectedNames = [
      "code_invariant",
      "config",
      "infrastructure",
      "data_ingress",
      "timing",
      "spec_drift",
      "observability",
      "out_of_scope",
    ];
    for (const name of expectedNames) {
      expect(prompt).toContain(name);
    }
  });

  // Test 6 (LOAD-BEARING): registering a 9th layer makes it appear in the next prompt
  it("registering a 9th fake layer makes it appear in the next classify prompt", async () => {
    // Register a mythical 9th layer
    registerRemediationLayer({
      name: "mythical",
      description: "A layer that does not exist in production.",
      promptHint: "Only use this in tests.",
      artifactKinds: ["documentation"],
    });

    expect(listRemediationLayers()).toHaveLength(9);

    // The prompt should now include "mythical"
    const layers = listRemediationLayers();
    const prompt = buildPrompt(makeSignal(), null, layers);
    expect(prompt).toContain("mythical");

    // And a classify call with a stub that returns "mythical" should succeed
    const mythicalResponse = JSON.stringify({
      primaryLayer: "mythical",
      secondaryLayers: [],
      artifacts: [{ kind: "documentation", rationale: "route to human" }],
      rationale: "This is a mythical bug.",
    });
    const stub = new StubLLMProvider(mythicalResponse);
    const plan = await classify(makeSignal(), null, stub);
    expect(plan.primaryLayer).toBe("mythical");
  });

  // Test 7: rationale field is populated from LLM response
  it("rationale is populated from the LLM response", async () => {
    const stub = new StubLLMProvider(makeCodeInvariantResponse());
    const plan = await classify(makeSignal(), makeLocus(), stub);

    expect(plan.rationale).toBe(
      "This is a logic error — divide by zero on empty array input.",
    );
  });

  // Test 8: locus: null produces a valid classification
  it("locus: null produces a valid classification without crashing", async () => {
    const stub = new StubLLMProvider(makeCodeInvariantResponse());
    const plan = await classify(makeSignal(), null, stub);

    expect(plan.primaryLayer).toBe("code_invariant");
    expect(plan.locus).toBeNull();
    expect(plan.signal).toBeDefined();
  });

  // Bonus: prompt includes "not resolved" when locus is null
  it("prompt contains 'not resolved' when locus is null", () => {
    const layers = listRemediationLayers();
    const prompt = buildPrompt(makeSignal(), null, layers);
    expect(prompt).toContain("not resolved");
  });

  // Bonus: prompt includes file:line when locus is provided
  it("prompt contains file:line when locus is provided", () => {
    const layers = listRemediationLayers();
    const locus = makeLocus({ file: "src/math.ts", line: 42 });
    const prompt = buildPrompt(makeSignal(), locus, layers);
    expect(prompt).toContain("src/math.ts:42");
  });
});
