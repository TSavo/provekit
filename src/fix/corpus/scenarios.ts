/**
 * Corpus scenario type definitions and helpers.
 *
 * A CorpusScenario is a self-contained bundle describing:
 *   - The synthetic project (files by relative path)
 *   - The bug report text
 *   - Canned LLM responses (pattern-matched on prompt substrings)
 *   - Expected outcomes (which stages complete, which fails, final outcome)
 */

export interface CorpusScenario {
  /** Unique ID — used in reports. */
  id: string;

  /** Bug class name (matches a principle's name OR "novel" for unprincipled bugs). */
  bugClass: string;

  /** Files in the synthetic project, keyed by relative path. */
  files: Record<string, string>;

  /** Bug report text — what the user would type in. */
  bugReport: string;

  /** Expected outcomes for this scenario. */
  expected: {
    /** Stages we expect to complete successfully (from the audit trail). */
    completes: string[];
    /** Stage we expect to fail (and why), or omit if expected to close cleanly. */
    fails?: { stage: string; reason: string };
    /** Final outcome: "applied" | "rejected" | "out_of_scope". */
    outcome: "applied" | "rejected" | "out_of_scope";
  };

  /**
   * Stub LLM responses. Pattern-match on prompt substrings (first match wins).
   * The runner builds a StubLLMProvider with these in insertion order.
   */
  llmResponses: { matchPrompt: string; response: string }[];
}

// ---------------------------------------------------------------------------
// Helper: build the Map<string, string> for StubLLMProvider from a scenario's
// llmResponses list.
// ---------------------------------------------------------------------------

export function buildResponseMap(
  responses: CorpusScenario["llmResponses"],
): Map<string, string> {
  const m = new Map<string, string>();
  for (const r of responses) {
    m.set(r.matchPrompt, r.response);
  }
  return m;
}
