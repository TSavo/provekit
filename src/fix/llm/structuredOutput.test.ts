/**
 * Tests for requestStructuredJson — the structured-output helper.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, writeFileSync, rmSync, mkdirSync, existsSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { requestStructuredJson, StructuredOutputError } from "./structuredOutput.js";
import { StubLLMProvider } from "../types.js";
import type { LLMProvider, AgentRequestOptions, AgentResult } from "../types.js";

// Save and restore PROVEKIT_AGENT_JSON across tests so the env-override path
// doesn't bleed between cases.
const ENV_KEY = "PROVEKIT_AGENT_JSON";

describe("requestStructuredJson", () => {
  let priorEnv: string | undefined;

  beforeEach(() => {
    priorEnv = process.env[ENV_KEY];
    delete process.env[ENV_KEY];
  });

  afterEach(() => {
    if (priorEnv === undefined) delete process.env[ENV_KEY];
    else process.env[ENV_KEY] = priorEnv;
  });

  // -------------------------------------------------------------------------
  // Text-mode (default) — preserves existing stub-based test behavior
  // -------------------------------------------------------------------------

  describe("text mode (default)", () => {
    it("parses JSON from a text-mode stub LLM", async () => {
      const stub = new StubLLMProvider(
        new Map([["test-prompt", '{"answer": 42}']]),
      );
      const result = await requestStructuredJson<{ answer: number }>({
        prompt: "test-prompt",
        llm: stub,
        stage: "unit",
      });
      expect(result.answer).toBe(42);
    });

    it("strips markdown fences via parseJsonFromLlm", async () => {
      const stub = new StubLLMProvider(
        new Map([["fenced", '```json\n{"x": 1}\n```']]),
      );
      const result = await requestStructuredJson<{ x: number }>({
        prompt: "fenced",
        llm: stub,
        stage: "unit",
      });
      expect(result.x).toBe(1);
    });

    it("runs schemaCheck on parsed value", async () => {
      const stub = new StubLLMProvider(
        new Map([["schemacheck", '{"kind": "ok", "value": 7}']]),
      );
      const schemaCheck = (parsed: unknown): { kind: "ok"; value: number } => {
        const p = parsed as Record<string, unknown>;
        if (p["kind"] !== "ok" || typeof p["value"] !== "number") {
          throw new Error("schema mismatch");
        }
        return { kind: "ok", value: p["value"] };
      };
      const result = await requestStructuredJson({
        prompt: "schemacheck",
        llm: stub,
        stage: "unit",
        schemaCheck,
      });
      expect(result).toEqual({ kind: "ok", value: 7 });
    });

    it("throws when schemaCheck rejects", async () => {
      const stub = new StubLLMProvider(
        new Map([["bad", '{"value": "not a number"}']]),
      );
      const schemaCheck = (parsed: unknown): { value: number } => {
        const p = parsed as Record<string, unknown>;
        if (typeof p["value"] !== "number") throw new Error("not a number");
        return { value: p["value"] };
      };
      await expect(
        requestStructuredJson({ prompt: "bad", llm: stub, stage: "unit", schemaCheck }),
      ).rejects.toThrow(/not a number/);
    });

    it("throws on non-JSON response", async () => {
      const stub = new StubLLMProvider(
        new Map([["broken", "this is not JSON"]]),
      );
      await expect(
        requestStructuredJson({ prompt: "broken", llm: stub, stage: "unit" }),
      ).rejects.toThrow(/parseJsonFromLlm/);
    });

    it("does NOT route to agent mode when llm.agent exists but useAgent is unspecified", async () => {
      // StubLLMProvider with agentResponses defines .agent. Default helper
      // behavior must keep using complete(), not the canned agent.
      const stub = new StubLLMProvider(
        new Map([["text-prompt", '{"path": "text"}']]),
        [{ matchPrompt: "text-prompt", fileEdits: [], text: "agent path" }],
      );
      const result = await requestStructuredJson<{ path: string }>({
        prompt: "text-prompt",
        llm: stub,
        stage: "unit",
      });
      expect(result.path).toBe("text");
    });
  });

  // -------------------------------------------------------------------------
  // Agent mode — useAgent: true with a fake provider
  // -------------------------------------------------------------------------

  describe("agent mode (useAgent: true)", () => {
    /** Build an LLM provider whose agent() writes a fixed JSON string to outputPath. */
    function makeWritingAgent(jsonContent: string, finalText = "wrote file"): LLMProvider {
      return {
        async complete() {
          throw new Error("complete should not be called in agent mode");
        },
        async agent(prompt: string, options: AgentRequestOptions): Promise<AgentResult> {
          // Extract the absolute output path the helper instructed us to use.
          // The instruction format is "  <path>\n" two lines after IMPORTANT.
          const match = prompt.match(/Write your JSON response to the absolute path:\s*\n\s*(\S+)/);
          if (!match || !match[1]) {
            throw new Error(`fake agent: could not find output path in prompt`);
          }
          const outputPath = match[1];
          mkdirSync(join(outputPath, ".."), { recursive: true });
          writeFileSync(outputPath, jsonContent, "utf-8");
          void options;
          return {
            filesChanged: [],
            diff: "",
            text: finalText,
            turnsUsed: 1,
            toolUses: [
              { id: "1", name: "Write", input: { file_path: outputPath, content: jsonContent }, isError: false, turn: 1, ms: 1, result: undefined },
            ],
            thinkingBlocks: [],
            textBlocks: [{ turn: 1, content: finalText }],
          };
        },
      };
    }

    it("reads JSON from the file the agent wrote", async () => {
      const llm = makeWritingAgent('{"value": 99}');
      const result = await requestStructuredJson<{ value: number }>({
        prompt: "ask",
        llm,
        stage: "unit",
        useAgent: true,
      });
      expect(result.value).toBe(99);
    });

    it("runs schemaCheck on agent-written JSON", async () => {
      const llm = makeWritingAgent('{"kind": "principle", "name": "X"}');
      const result = await requestStructuredJson<{ kind: string; name: string }>({
        prompt: "ask",
        llm,
        stage: "unit",
        useAgent: true,
        schemaCheck: (parsed: unknown) => {
          const p = parsed as Record<string, unknown>;
          if (typeof p["kind"] !== "string" || typeof p["name"] !== "string") {
            throw new Error("missing kind/name");
          }
          return { kind: p["kind"], name: p["name"] };
        },
      });
      expect(result).toEqual({ kind: "principle", name: "X" });
    });

    it("falls back to inline JSON when agent didn't use Write tool but text contains valid JSON", async () => {
      // LLM nondeterminism: even when the prompt says "Write JSON to {path}",
      // the LLM occasionally returns the JSON inline in text. Resilience
      // backstop: extract JSON from text rather than aborting the run.
      const inlineJsonAgent: LLMProvider = {
        async complete() { throw new Error("not called"); },
        async agent() {
          return {
            filesChanged: [],
            diff: "",
            text: "I'll write the file. Here it is: ```json\n{\"recovered\": true, \"value\": 42}\n```",
            turnsUsed: 1,
            toolUses: [],
            thinkingBlocks: [],
            textBlocks: [],
          };
        },
      };

      const result = await requestStructuredJson<{ recovered: boolean; value: number }>({
        prompt: "ask",
        llm: inlineJsonAgent,
        stage: "unit",
        useAgent: true,
      });
      expect(result.recovered).toBe(true);
      expect(result.value).toBe(42);
    });

    it("throws StructuredOutputError when agent returns without writing AND text has no extractable JSON", async () => {
      const unextractableAgent: LLMProvider = {
        async complete() { throw new Error("not called"); },
        async agent() {
          return {
            filesChanged: [],
            diff: "",
            text: "I encountered an error and couldn't produce a result.",
            turnsUsed: 1,
            toolUses: [],
            thinkingBlocks: [],
            textBlocks: [],
          };
        },
      };

      let caught: StructuredOutputError | undefined;
      try {
        await requestStructuredJson({
          prompt: "ask",
          llm: unextractableAgent,
          stage: "unit",
          useAgent: true,
        });
      } catch (err) {
        caught = err as StructuredOutputError;
      }
      expect(caught).toBeInstanceOf(StructuredOutputError);
      expect(caught!.message).toMatch(/did not write JSON file/);
      expect(caught!.message).toMatch(/inline JSON could not be extracted/);
      expect(caught!.scratchPath).toBeDefined();
      expect(existsSync(caught!.scratchPath!)).toBe(true);
      rmSync(caught!.scratchPath!, { recursive: true, force: true });
    });

    it("throws StructuredOutputError when agent writes invalid JSON", async () => {
      const llm = makeWritingAgent("this is not json at all");
      let caught: StructuredOutputError | undefined;
      try {
        await requestStructuredJson({
          prompt: "ask",
          llm,
          stage: "unit",
          useAgent: true,
        });
      } catch (err) {
        caught = err as StructuredOutputError;
      }
      expect(caught).toBeInstanceOf(StructuredOutputError);
      expect(caught!.message).toMatch(/JSON\.parse failed/);
      expect(caught!.fileContent).toBe("this is not json at all");
      expect(caught!.scratchPath).toBeDefined();
      // Cleanup.
      if (caught?.scratchPath) {
        rmSync(caught.scratchPath, { recursive: true, force: true });
      }
    });

    it("falls back to text mode when useAgent: true but llm.agent is undefined", async () => {
      const stub = new StubLLMProvider(
        new Map([["fallback", '{"mode": "text"}']]),
      );
      const result = await requestStructuredJson<{ mode: string }>({
        prompt: "fallback",
        llm: stub,
        stage: "unit",
        useAgent: true,
      });
      expect(result.mode).toBe("text");
    });

    it("env PROVEKIT_AGENT_JSON=1 enables agent mode without explicit useAgent", async () => {
      process.env[ENV_KEY] = "1";
      const llm = makeWritingAgent('{"via": "env"}');
      const result = await requestStructuredJson<{ via: string }>({
        prompt: "ask",
        llm,
        stage: "unit",
      });
      expect(result.via).toBe("env");
    });

    it("explicit useAgent: false beats env PROVEKIT_AGENT_JSON=1", async () => {
      process.env[ENV_KEY] = "1";
      const stub = new StubLLMProvider(
        new Map([["text-prompt", '{"win": "explicit"}']]),
        [{ matchPrompt: "text-prompt", fileEdits: [], text: "should not run" }],
      );
      const result = await requestStructuredJson<{ win: string }>({
        prompt: "text-prompt",
        llm: stub,
        stage: "unit",
        useAgent: false,
      });
      expect(result.win).toBe("explicit");
    });

    it("uses caller-supplied scratch dir and does not delete it", async () => {
      const scratchDir = mkdtempSync(join(tmpdir(), "provekit-test-explicit-cwd-"));
      try {
        const llm = makeWritingAgent('{"kept": true}');
        const result = await requestStructuredJson<{ kept: boolean }>({
          prompt: "ask",
          llm,
          stage: "unit",
          useAgent: true,
          cwd: scratchDir,
        });
        expect(result.kept).toBe(true);
        // Caller-supplied dir is left in place for the caller to manage.
        expect(existsSync(scratchDir)).toBe(true);
      } finally {
        rmSync(scratchDir, { recursive: true, force: true });
      }
    });
  });
});
