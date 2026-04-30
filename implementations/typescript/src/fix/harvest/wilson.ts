/**
 * Wilson score interval for a Bernoulli proportion (precision/recall
 * confidence intervals on small held-out samples). Used by #115 step 5
 * to put error bars on the test-set precision number.
 *
 * Why Wilson, not Wald: Wald (the textbook normal-approx) is bad for
 * extreme proportions (p=0 or p=1) and small n — gives intervals that
 * extend below 0 or above 1. Wilson stays in [0,1] regardless and is
 * better-calibrated at small n. It's the standard for this exact case
 * (1-2/10 vs 9-10/10 precision claims on ~30-100 candidates).
 *
 * Reference: Wilson, E. B. (1927). Probable inference, the law of
 * succession, and statistical inference. JASA 22:209-212.
 */

export interface WilsonInterval {
  /** Point estimate (k / n). */
  pointEstimate: number;
  /** Lower bound of the confidence interval. */
  lower: number;
  /** Upper bound of the confidence interval. */
  upper: number;
  /** Sample size used. */
  n: number;
  /** Successes observed. */
  k: number;
  /** Confidence level (e.g. 0.95). */
  confidence: number;
}

/**
 * z-score for two-sided confidence interval. Hard-coded common levels;
 * we don't need a full quantile inverse.
 */
function zForConfidence(confidence: number): number {
  if (confidence === 0.90) return 1.6448536269514722;
  if (confidence === 0.95) return 1.959963984540054;
  if (confidence === 0.99) return 2.5758293035489004;
  // For other levels: rational approximation of Φ^(-1)((1+confidence)/2).
  // Limited use — most callers want 95%.
  throw new Error(`unsupported confidence level: ${confidence} (use 0.90, 0.95, or 0.99)`);
}

/**
 * Compute Wilson score interval for k successes in n trials at the
 * given confidence (default 0.95).
 *
 * Edge cases:
 *   - n = 0 → returns NaN bounds and pointEstimate = 0
 *   - k = 0 or k = n → bounds correctly stay in [0,1] (this is the
 *     reason to use Wilson over Wald in the first place)
 */
export function wilson(k: number, n: number, confidence = 0.95): WilsonInterval {
  if (n === 0) {
    return { pointEstimate: 0, lower: NaN, upper: NaN, n, k, confidence };
  }
  if (k < 0 || k > n) {
    throw new Error(`invalid Wilson inputs: k=${k}, n=${n}`);
  }
  const p = k / n;
  const z = zForConfidence(confidence);
  const z2 = z * z;
  const denom = 1 + z2 / n;
  const center = (p + z2 / (2 * n)) / denom;
  const halfWidth = (z * Math.sqrt((p * (1 - p) + z2 / (4 * n)) / n)) / denom;
  return {
    pointEstimate: p,
    lower: Math.max(0, center - halfWidth),
    upper: Math.min(1, center + halfWidth),
    n,
    k,
    confidence,
  };
}

/** Format a Wilson interval as "p̂=0.733 [0.493, 0.876] (k=11/15, 95% CI)". */
export function formatWilson(w: WilsonInterval): string {
  if (Number.isNaN(w.lower)) return `n=0 (no data)`;
  return `p̂=${w.pointEstimate.toFixed(3)} [${w.lower.toFixed(3)}, ${w.upper.toFixed(3)}] (k=${w.k}/${w.n}, ${(w.confidence * 100).toFixed(0)}% CI)`;
}
