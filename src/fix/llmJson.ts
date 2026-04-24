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
