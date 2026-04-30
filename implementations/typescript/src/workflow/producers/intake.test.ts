import { describe, it, expect, beforeEach } from "vitest";
import { mkdtempSync, mkdirSync } from "fs";
import { tmpdir } from "os";
import { join, dirname } from "path";
import { fileURLToPath } from "url";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { openDb } from "../../db/index.js";
import { _clearIntakeRegistry } from "../../fix/intake.js";
import { registerAll } from "../../fix/intakeAdapters/index.js";
import { StubLLMProvider } from "../../fix/types.js";
import { WorkflowRunner } from "../runner.js";
import { InMemoryRegistry } from "../registry.js";
import { parseManifest, runManifest, manifestToWorkflow } from "../manifest.js";
import { makeIntakeStage, INTAKE_CAPABILITY } from "./intake.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const DRIZZLE_FOLDER = join(__dirname, "..", "..", "..", "drizzle");

beforeEach(() => {
  _clearIntakeRegistry();
  registerAll();
});

function makeDb() {
  const tmp = mkdtempSync(join(tmpdir(), "intake-stage-"));
  mkdirSync(join(tmp, ".provekit"), { recursive: true });
  const db = openDb(join(tmp, ".provekit", "test.db"));
  migrate(db, { migrationsFolder: DRIZZLE_FOLDER });
  return db;
}

function reportLlm(): StubLLMProvider {
  return new StubLLMProvider(
    new Map([
      [
        "Bug report",
        JSON.stringify({
          summary: "Division crashes when denominator is 0.",
          failureDescription: "Division-by-zero in calculate.",
          fixHint: "Guard before dividing.",
          codeReferences: [{ file: "src/math.ts", line: 42, function: "calculate" }],
          bugClassHint: "divide-by-zero",
        }),
      ],
    ]),
  );
}

const wf = { name: "test-wf", cid: "wf-intake-test-v1" };

describe("intake Stage", () => {
  it("runs through WorkflowRunner.runStage and returns an IntentSignal", async () => {
    const db = makeDb();
    const stage = makeIntakeStage(reportLlm());
    const runner = new WorkflowRunner(db, wf);

    const result = await runner.runStage(stage, {
      text: "Division crashes when denominator is 0.",
      source: "report",
    });

    expect(result.cacheHit).toBe(false);
    expect(result.output.summary).toMatch(/Division crashes/);
    expect(result.output.codeReferences).toHaveLength(1);
    expect(result.output.codeReferences[0].file).toBe("src/math.ts");
  });

  it("caches identical input — second run is a hit, LLM not invoked", async () => {
    const db = makeDb();
    let llmCalls = 0;
    const baseLlm = reportLlm();
    const wrappedLlm = {
      ...baseLlm,
      async complete(params: Parameters<typeof baseLlm.complete>[0]) {
        llmCalls++;
        return baseLlm.complete(params);
      },
    };
    const stage = makeIntakeStage(wrappedLlm);
    const runner = new WorkflowRunner(db, wf);

    const a = await runner.runStage(stage, { text: "Division crashes.", source: "report" });
    expect(a.cacheHit).toBe(false);
    expect(llmCalls).toBe(1);

    const b = await runner.runStage(stage, { text: "Division crashes.", source: "report" });
    expect(b.cacheHit).toBe(true);
    expect(b.cid).toBe(a.cid);
    expect(b.output.summary).toBe(a.output.summary);
    expect(llmCalls).toBe(1); // no second invocation — pure DB read
  });

  it("dispatches via the registry as capability 'intake'", async () => {
    const db = makeDb();
    const stage = makeIntakeStage(reportLlm());
    const registry = new InMemoryRegistry();
    registry.register(INTAKE_CAPABILITY, stage);

    const runner = new WorkflowRunner(db, wf, registry);
    const result = await runner.request<{ text: string; source: string }, ReturnType<typeof stage.deserializeOutput>>(
      INTAKE_CAPABILITY,
      { text: "Division crashes when denominator is 0.", source: "report" },
    );

    expect(result.output.summary).toMatch(/Division crashes/);
  });

  it("runs as the first node in a YAML-driven workflow", async () => {
    const db = makeDb();
    const stage = makeIntakeStage(reportLlm());
    const registry = new InMemoryRegistry();
    registry.register(INTAKE_CAPABILITY, stage);

    const yaml = `
name: intake-only
cid: wf-intake-only-v1
description: minimal one-node workflow that intakes and stops.
nodes:
  - id: intake
    capability: intake
    input:
      text: $input.text
      source: $input.source
output: $node.intake.output
`;
    const manifest = parseManifest(yaml);
    const runner = new WorkflowRunner(db, manifestToWorkflow(manifest), registry);
    const result = await runManifest(runner, registry, manifest, {
      text: "Division crashes when denominator is 0.",
      source: "report",
    });

    const signal = result.output as { summary: string; codeReferences: unknown[] };
    expect(signal.summary).toMatch(/Division crashes/);
    expect(signal.codeReferences).toHaveLength(1);
  });
});
