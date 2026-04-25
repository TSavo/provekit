/**
 * Structured-output helper.
 *
 * Architecture: stop parsing JSON out of LLM prose. Instead, instruct the LLM
 * agent to write the structured output to a known file via the Write tool,
 * and read the file back. Eliminates the entire class of "LLM wrapped output
 * in fences / prefixed with prose / appended a comment" failures.
 *
 * Two paths:
 *
 *   AGENT mode  (opts.useAgent === true OR env PROVEKIT_AGENT_JSON=1, AND
 *                llm.agent is defined):
 *     - Create a scratch dir under tmpdir().
 *     - Append a strict instruction to the prompt: "Write your JSON to <path>
 *       using the Write tool. Do not include the JSON in your text response."
 *     - Call llm.agent(...) with allowedTools restricted to Write.
 *     - Read the file. JSON.parse + optional schemaCheck.
 *     - Cleanup scratch on success. Preserve on error and log the path so a
 *       human can inspect the malformed output.
 *
 *   TEXT mode  (default, and the only path StubLLMProvider tests use):
 *     - llm.complete({prompt, model}). Run the legacy tolerant parser.
 *     - The legacy parser strips ``` fences, trims whitespace, then JSON.parse.
 *     - This preserves backward compat with all existing stub tests.
 *
 * Why opt-in (not auto-detect on llm.agent presence): StubLLMProvider has
 * `.agent` defined whenever a test passes agentResponses. Those agent
 * responses are tailored to specific call sites (e.g., candidateGen). If the
 * helper auto-routed every JSON request to agent mode, the stub would either
 * return the wrong canned fileEdits or throw "no canned response." Opt-in
 * keeps stub-mode tests stable while letting real-LLM runs flip every site
 * via the env var.
 */

import { mkdtempSync, existsSync, readFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import type { LLMProvider } from "../types.js";
import type { FixLoopLogger } from "../logger.js";
import { parseJsonFromLlm } from "../llmJson.js";

/**
 * Thrown when the agent fails to produce a parseable JSON file.
 * Carries the agent's text response (if any) and the file content (if any)
 * so the caller and logs can see what went wrong.
 */
export class StructuredOutputError extends Error {
  constructor(
    message: string,
    public readonly agentText?: string,
    public readonly fileContent?: string,
    public readonly scratchPath?: string,
  ) {
    super(message);
    this.name = "StructuredOutputError";
  }
}

export interface StructuredJsonOptions<T> {
  /** Prompt the LLM should answer (helper appends file-write instruction in agent mode). */
  prompt: string;
  /** LLM provider. Same instance used by every other call site. */
  llm: LLMProvider;
  /** Stage label, used for scratch dir naming + logger context. */
  stage: string;
  /**
   * Force agent mode. Default: false unless env PROVEKIT_AGENT_JSON=1.
   * Even when true, falls back to text mode if llm.agent is undefined.
   */
  useAgent?: boolean;
  /** Override scratch directory. Default: mkdtempSync under tmpdir(). */
  cwd?: string;
  /** Model tier override forwarded on the text-fallback path. */
  model?: "haiku" | "sonnet" | "opus";
  /** JSON schema forwarded on the text-fallback path (used by classify). */
  schema?: object;
  /**
   * Optional schema check. Throws to reject. Returns the typed value on
   * success. This is the SAME shape as the per-site validators that
   * currently live next to parseJsonFromLlm calls.
   */
  schemaCheck?: (parsed: unknown) => T;
  /** Logger; if provided, emits prompt/response/llmCall events. */
  logger?: FixLoopLogger;
  /**
   * Overlay/working dir for agent-mode invocation. The scratch dir for the
   * output file is always separate from this. Provided to the agent SDK as
   * `cwd` so its tool calls are confined here. If unset, defaults to a
   * sibling of the scratch dir. Most callers never need to set this — the
   * helper's instruction is "write to an absolute path", not "edit project
   * files."
   */
  agentCwd?: string;
}

/**
 * Determine whether agent mode is active for this call.
 * Order: explicit useAgent → env override → text mode.
 */
function shouldUseAgent(opts: { useAgent?: boolean }, llm: LLMProvider): boolean {
  if (!llm.agent) return false;
  if (opts.useAgent === true) return true;
  if (opts.useAgent === false) return false;
  return process.env["PROVEKIT_AGENT_JSON"] === "1";
}

/**
 * Build the suffix the helper appends to the user prompt in agent mode.
 * The instruction is intentionally explicit — Write tool, absolute path,
 * no JSON in the text response — to maximise the odds Claude does the right
 * thing in one turn.
 */
function buildAgentInstruction(outputPath: string): string {
  return (
    `\n\nIMPORTANT: Write your JSON response to the absolute path:\n` +
    `  ${outputPath}\n` +
    `using the Write tool. The file content must be ONLY the JSON object — ` +
    `no markdown fences, no prose, no commentary. Do not include the JSON in ` +
    `your text response. After writing the file, you may briefly confirm in ` +
    `plain text (one sentence) that you wrote the file. Do not perform any ` +
    `other actions.`
  );
}

/**
 * Request a structured JSON response from the LLM and return it parsed.
 *
 * In agent mode: the agent writes a JSON file via the Write tool; helper
 * reads + parses + (optionally) schema-checks. In text mode: helper calls
 * llm.complete(...) and runs the legacy tolerant parser.
 *
 * Both paths run schemaCheck if provided. Both paths log via the logger if
 * provided. Both paths throw on parse/validation failure with enough context
 * (raw response, file content, scratch path) to debug.
 */
export async function requestStructuredJson<T = unknown>(
  opts: StructuredJsonOptions<T>,
): Promise<T> {
  const { prompt, llm, stage, model, schema, schemaCheck, logger } = opts;

  if (shouldUseAgent(opts, llm)) {
    return runAgentMode<T>({ prompt, llm, stage, model, schemaCheck, logger, agentCwd: opts.agentCwd, cwd: opts.cwd });
  }

  return runTextMode<T>({ prompt, llm, stage, model, schema, schemaCheck, logger });
}

// ---------------------------------------------------------------------------
// Agent-mode path
// ---------------------------------------------------------------------------

async function runAgentMode<T>(args: {
  prompt: string;
  llm: LLMProvider;
  stage: string;
  model?: "haiku" | "sonnet" | "opus";
  schemaCheck?: (parsed: unknown) => T;
  logger?: FixLoopLogger;
  agentCwd?: string;
  cwd?: string;
}): Promise<T> {
  const { prompt, llm, stage, model, schemaCheck, logger } = args;

  // llm.agent is guaranteed non-null by shouldUseAgent.
  const agent = llm.agent!;

  // Scratch dir: holds output.json. Separate from agentCwd so the file write
  // never touches project files even if agentCwd is the project root.
  const scratchDir = args.cwd ?? mkdtempSync(join(tmpdir(), `provekit-${stage}-`));
  const outputPath = join(scratchDir, "output.json");

  const mutatedPrompt = prompt + buildAgentInstruction(outputPath);

  // The agent's working dir. The instruction uses an absolute output path so
  // the agent doesn't need a meaningful cwd; we default to scratchDir to keep
  // any stray Read/Write attempts contained.
  const cwd = args.agentCwd ?? scratchDir;

  const t0 = Date.now();
  logger?.prompt(stage, model ?? "agent", mutatedPrompt);

  let agentText = "";
  try {
    const result = await agent(mutatedPrompt, {
      cwd,
      allowedTools: ["Write"],
      maxTurns: 3,
      model,
    });
    agentText = result.text ?? "";

    logger?.llmCall({
      stage,
      model: model ?? "agent",
      promptLen: mutatedPrompt.length,
      responseLen: agentText.length,
      durationMs: Date.now() - t0,
    });
    logger?.response(stage, model ?? "agent", agentText);
  } catch (err) {
    // Preserve scratch dir on error so a human can inspect.
    throw new StructuredOutputError(
      `[${stage}] LLM agent call failed: ${err instanceof Error ? err.message : String(err)}. scratch=${scratchDir}`,
      undefined,
      undefined,
      scratchDir,
    );
  }

  if (!existsSync(outputPath)) {
    // Preserve scratch dir for inspection. Do NOT fall back to text-mode
    // parse — failure here is the signal the prompt instruction needs
    // tightening, not a parser-leniency problem.
    throw new StructuredOutputError(
      `[${stage}] LLM agent did not write JSON file at ${outputPath}. scratch=${scratchDir}. ` +
      `Agent text response: ${agentText.slice(0, 500)}`,
      agentText,
      undefined,
      scratchDir,
    );
  }

  const fileContent = readFileSync(outputPath, "utf-8");

  let parsed: unknown;
  try {
    parsed = JSON.parse(fileContent);
  } catch (err) {
    throw new StructuredOutputError(
      `[${stage}] JSON.parse failed on agent-written file: ${err instanceof Error ? err.message : String(err)}. ` +
      `scratch=${scratchDir}. File content (truncated to 500 chars): ${fileContent.slice(0, 500)}`,
      agentText,
      fileContent,
      scratchDir,
    );
  }

  let typed: T;
  try {
    typed = schemaCheck ? schemaCheck(parsed) : (parsed as T);
  } catch (err) {
    throw new StructuredOutputError(
      `[${stage}] schemaCheck rejected agent output: ${err instanceof Error ? err.message : String(err)}. scratch=${scratchDir}`,
      agentText,
      fileContent,
      scratchDir,
    );
  }

  // Success: clean up scratch dir.
  if (!args.cwd) {
    try { rmSync(scratchDir, { recursive: true, force: true }); } catch { /* ignore */ }
  }

  return typed;
}

// ---------------------------------------------------------------------------
// Text-mode path (legacy, used by every stub test)
// ---------------------------------------------------------------------------

async function runTextMode<T>(args: {
  prompt: string;
  llm: LLMProvider;
  stage: string;
  model?: "haiku" | "sonnet" | "opus";
  schema?: object;
  schemaCheck?: (parsed: unknown) => T;
  logger?: FixLoopLogger;
}): Promise<T> {
  const { prompt, llm, stage, model, schema, schemaCheck, logger } = args;

  const t0 = Date.now();
  logger?.prompt(stage, model ?? "sonnet", prompt);

  const completeArgs: { prompt: string; model?: "haiku" | "sonnet" | "opus"; schema?: object } = { prompt };
  if (model !== undefined) completeArgs.model = model;
  if (schema !== undefined) completeArgs.schema = schema;

  const raw = await llm.complete(completeArgs);

  logger?.llmCall({
    stage,
    model: model ?? "sonnet",
    promptLen: prompt.length,
    responseLen: raw.length,
    durationMs: Date.now() - t0,
  });
  logger?.response(stage, model ?? "sonnet", raw);

  // Parse with the legacy tolerant parser. It strips fences and trims
  // whitespace, but does NOT tolerate prose-prefix; schemaCheck still runs
  // after.
  const parsed = parseJsonFromLlm<unknown>(raw, stage);

  if (schemaCheck) {
    return schemaCheck(parsed);
  }
  return parsed as T;
}
