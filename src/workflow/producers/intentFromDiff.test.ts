import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { StubLLMProvider } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { makeIntentFromDiffStage } from "./intentFromDiff.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "intent-from-diff-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

const wf = { name: "test-wf", cid: "wf-intent-from-diff-test-v1" };

const DIVZERO_DIFF = [
  "--- a/src/math.ts",
  "+++ b/src/math.ts",
  "@@ -1,3 +1,5 @@",
  " function calculate(a, b) {",
  "+  if (b === 0) return { error: 'divide by zero' };",
  "   return a / b;",
  " }",
].join("\n");

const ARITY_DIFF = [
  "--- a/src/list.ts",
  "+++ b/src/list.ts",
  "@@ -1,3 +1,4 @@",
  " function head(xs) {",
  "+  if (xs.length === 0) throw new Error('empty');",
  "   return xs[0];",
  " }",
].join("\n");

// StubLLMProvider matches by substring on the rendered prompt.
// "extract what property the developer" is a stable phrase from
// PROMPT_TEMPLATE in intentFromDiff.ts; key the stub on it.
const STUB_PROMPT_MATCH = "extract what property the developer";

function divzeroLlm(): StubLLMProvider {
  return new StubLLMProvider(
    new Map([
      [
        STUB_PROMPT_MATCH,
        JSON.stringify({
          inferredIntent:
            "function calculate must not be called with a zero denominator",
          proposedIrFormula:
            "forAll(call: CalculateCall) => call.b !== 0 OR call.returnsError()",
          confidence: 0.92,
          rationale:
            "The added guard clause encodes the invariant: when b is zero the function returns an error rather than dividing.",
        }),
      ],
    ]),
  );
}

const DEFAULT_DEPS = (llm: StubLLMProvider) => ({
  llm,
  promptCid: "a".repeat(32),
  llmIdentifier: "claude-opus@4-7",
});

describe("intent-from-diff Stage", () => {
  it("populates proposedIrFormula and confidence from the stub LLM response", async () => {
    const db = makeDb();
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(divzeroLlm()));
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash in calculate()",
      linkedTickets: [
        { id: "INC-2847", summary: "calculate() crashes when b=0" },
      ],
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.proposedIrFormula).toMatch(/CalculateCall/);
    expect(result.output.confidence).toBe(0.92);
    expect(result.output.inferredIntent).toMatch(/zero denominator/);
    expect(result.output.llm).toBe("claude-opus");
    expect(result.output.llmVersion).toBe("4-7");
    expect(result.output.promptCid).toBe("a".repeat(32));
  });

  it("caches identical input — second run is a hit, LLM not invoked again", async () => {
    const db = makeDb();
    let llmCalls = 0;
    const baseLlm = divzeroLlm();
    const wrappedLlm = {
      ...baseLlm,
      async complete(params: Parameters<typeof baseLlm.complete>[0]) {
        llmCalls++;
        return baseLlm.complete(params);
      },
    };
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(wrappedLlm));
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });
    expect(a.cacheHit).toBe(false);
    expect(llmCalls).toBe(1);

    const b = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.proposedIrFormula).toBe(a.output.proposedIrFormula);
    expect(b.output.inferredIntent).toBe(a.output.inferredIntent);
    expect(llmCalls).toBe(1);
  });

  it("different diff → different memento (cache miss)", async () => {
    const db = makeDb();
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(divzeroLlm()));
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });
    const b = await runner.runStage(stage, {
      diff: ARITY_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(false);
    expect(b.cid).not.toBe(a.cid);
  });

  it("different commit message → different memento (cache miss)", async () => {
    const db = makeDb();
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(divzeroLlm()));
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });
    const b = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "guard against zero denominator",
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(false);
    expect(b.cid).not.toBe(a.cid);
  });

  it("different promptCid → different producedBy → cache miss (prompt revision)", async () => {
    const db = makeDb();
    const runner = new WorkflowRunner(db, wf);

    const stageV1 = makeIntentFromDiffStage({
      llm: divzeroLlm(),
      promptCid: "a".repeat(32),
      llmIdentifier: "claude-opus@4-7",
    });
    const stageV2 = makeIntentFromDiffStage({
      llm: divzeroLlm(),
      promptCid: "b".repeat(32),
      llmIdentifier: "claude-opus@4-7",
    });

    expect(stageV1.producedBy).not.toBe(stageV2.producedBy);

    const a = await runner.runStage(stageV1, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });
    const b = await runner.runStage(stageV2, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });

    expect(a.cacheHit).toBe(false);
    expect(b.cacheHit).toBe(false);
    expect(b.cid).not.toBe(a.cid);
  });

  it("witness shape matches LlmProposalEvidence body and round-trips exactly", async () => {
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(divzeroLlm()));
    const proposal = {
      proposedIrFormula:
        "forAll(call: CalculateCall) => call.b !== 0 OR call.returnsError()",
      confidence: 0.92,
      rationale: "guard clause encodes the invariant",
      inferredIntent: "calculate must not be called with zero denominator",
      llm: "claude-opus",
      llmVersion: "4-7",
      promptCid: "a".repeat(32),
    };

    const witness = stage.serializeOutput(proposal);
    const parsed = JSON.parse(witness);

    // LlmProposalEvidence body fields are present at the top level.
    expect(parsed.llm).toBe("claude-opus");
    expect(parsed.llmVersion).toBe("4-7");
    expect(parsed.promptCid).toBe("a".repeat(32));
    expect(parsed.proposedIrFormula).toBe(proposal.proposedIrFormula);
    expect(parsed.confidence).toBe(0.92);
    expect(parsed.rationale).toBe("guard clause encodes the invariant");

    // Round-trip is exact.
    expect(stage.deserializeOutput(witness)).toEqual(proposal);
  });

  it("throws a debuggable error when the LLM returns non-JSON", async () => {
    const db = makeDb();
    const llm = new StubLLMProvider(
      new Map([[STUB_PROMPT_MATCH, "this is not JSON"]]),
    );
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(llm));
    const runner = new WorkflowRunner(db, wf);

    await expect(
      runner.runStage(stage, {
        diff: DIVZERO_DIFF,
        commitMessage: "fix",
      }),
    ).rejects.toThrow(/this is not JSON/);
  });

  it("works without linkedTickets and without hostLanguageHint", async () => {
    const db = makeDb();
    const stage = makeIntentFromDiffStage(DEFAULT_DEPS(divzeroLlm()));
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      diff: DIVZERO_DIFF,
      commitMessage: "fix divide-by-zero crash",
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.proposedIrFormula).toMatch(/CalculateCall/);
  });
});
