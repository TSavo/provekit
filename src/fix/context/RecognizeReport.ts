/**
 * The artifact produced by B3 (Recognize).
 *
 * B3 runs every library principle's compiled query against the locus.
 * If a principle matches AND has a fixTemplate + testTemplate, the
 * loop short-circuits to the mechanical mode (C1m/C3m/C5m/C6m) — no
 * LLM calls. If nothing matched, the LLM-driven C-stages take over.
 *
 * The wrapper preserves RecognizeResult and adds a confidence band:
 * mechanical-mode lands very high confidence (the principle was
 * authored deliberately + validated); LLM-mode requires a separate
 * confidence assessment from each downstream stage.
 */

import type { RecognizeResult } from "../stages/recognize.js";

export interface RecognizeReport {
  /** True if any principle matched at the locus with full templates. */
  readonly matched: boolean;
  /** Underlying recognize result — RecognitionResult for full library state. */
  readonly result: RecognizeResult;
  /** Confidence: mechanical hits are deterministic ("high"); misses leave it null. */
  readonly confidence: "high" | null;
}
