/**
 * Parse a JSON response from an LLM, tolerating markdown code-fencing.
 *
 * LLMs (especially smaller tiers) frequently wrap JSON responses in ```json
 * ... ``` fences despite "JSON only" prompts. This helper strips those
 * before JSON.parse, plus common no-ops like leading/trailing whitespace.
 *
 * On parse failure, throws with the raw response in the error message
 * (truncated to 500 chars) so debugging doesn't require re-running.
 */
export function parseJsonFromLlm<T = unknown>(raw: string, context?: string): T {
  let cleaned = raw.trim();

  // Strip ```json or ``` fences
  if (cleaned.startsWith("```")) {
    // Drop first line (```json or ```)
    const firstNewline = cleaned.indexOf("\n");
    if (firstNewline !== -1) {
      cleaned = cleaned.slice(firstNewline + 1);
    }
  }
  // Strip trailing ```
  if (cleaned.endsWith("```")) {
    cleaned = cleaned.slice(0, -3).trim();
  }

  try {
    return JSON.parse(cleaned) as T;
  } catch (e) {
    throw new Error(
      `parseJsonFromLlm${context ? ` [${context}]` : ""}: JSON.parse failed: ${e instanceof Error ? e.message : String(e)}\n` +
      `Raw response (truncated to 500 chars):\n${raw.slice(0, 500)}`,
    );
  }
}

/**
 * Aggressive JSON extraction from arbitrary LLM text. Used as a resilience
 * fallback when an agent was supposed to write JSON to a file via the Write
 * tool but instead returned the JSON inline in its text response.
 *
 * Try in order:
 *   1. parseJsonFromLlm(raw) — handles bare JSON and ```fenced``` JSON
 *   2. Find a triple-backtick fenced block anywhere in the text and parse it
 *   3. Find the last balanced `{...}` or `[...]` substring and parse it
 *
 * Returns null if nothing parses. Callers decide whether to throw or proceed.
 *
 * NOT a replacement for the Write-tool contract — only a backstop for when
 * the LLM disobeys it. The contract still expresses the intent; this helper
 * keeps a single LLM disobedience from aborting an entire run.
 */
export function extractJsonFromText(raw: string): unknown | null {
  // 1. Bare or simple-fenced
  try {
    return parseJsonFromLlm(raw);
  } catch {
    /* fall through */
  }

  // 2. Find any ```...``` block (json fence or unmarked) and try parsing it
  const fenceMatch = raw.match(/```(?:json)?\s*([\s\S]*?)\s*```/);
  if (fenceMatch && fenceMatch[1]) {
    try {
      return JSON.parse(fenceMatch[1].trim());
    } catch {
      /* fall through */
    }
  }

  // 3. Last balanced JSON object or array in the text. Walk from the last
  //    `{` or `[`, find its matching close, attempt parse, retry with the
  //    next-earlier brace if it fails.
  for (let i = raw.length - 1; i >= 0; i--) {
    const ch = raw[i];
    if (ch !== "}" && ch !== "]") continue;
    const open = ch === "}" ? "{" : "[";
    let depth = 0;
    for (let j = i; j >= 0; j--) {
      if (raw[j] === ch) depth++;
      else if (raw[j] === open) {
        depth--;
        if (depth === 0) {
          try {
            return JSON.parse(raw.slice(j, i + 1));
          } catch {
            break;
          }
        }
      }
    }
  }

  return null;
}
