/**
 * Step 2 of the standing-invariant-runtime spec: re-resolve bindings,
 * report decay. No path enumeration or Z3 yet — those land in steps 3-5.
 *
 * Decay categories (v1, smallest useful surface):
 *   - DECAYED_DELETED: file the binding pointed at no longer exists
 *   - DECAYED_CHANGED: file exists, but the line range is out of bounds,
 *                      or its content doesn't match the recorded node hash
 *   - HOLDS: bindings resolve cleanly. (v1 stops here — Z3 path checking
 *            in step 5 will also need to pass for a full hold verdict.)
 *
 * v1 limitation: bindingNodeHashes were empty when the dogfood loop
 * persisted invariants (the orchestrator didn't have substrate access at
 * write time). Decay therefore reduces to "file exists" + "line range
 * still in bounds." Step 3 (path enumerator) will land alongside the
 * substrate-aware binding writer that populates real node hashes; this
 * verifier upgrades transparently to content-hash decay detection at
 * that point.
 */

import { existsSync, readFileSync } from "fs";
import type { StoredInvariant } from "./invariantStore.js";
import { readInvariants } from "./invariantStore.js";

// ---------------------------------------------------------------------------
// Verdict types
// ---------------------------------------------------------------------------

export type DecayKind = "deleted" | "changed";

export interface BindingResolution {
  smt_constant: string;
  status: "resolved" | "decayed";
  decayKind?: DecayKind;
  reason?: string;
}

export interface InvariantVerdict {
  invariant: StoredInvariant;
  status: "holds" | "decayed";
  bindings: BindingResolution[];
  /**
   * v1 leaves Z3 path checking out. When step 5 lands, this field will
   * either remain "holds" (no path can violate the invariant) or flip
   * to "violated" with a Z3 model attached.
   */
  pathCheck: "skipped" | "holds" | "violated";
}

export interface VerifyReport {
  verdicts: InvariantVerdict[];
  /** Counts for human-readable summary + exit-code derivation. */
  summary: {
    total: number;
    holds: number;
    decayed: number;
    violated: number;
  };
}

// ---------------------------------------------------------------------------
// Binding resolution
// ---------------------------------------------------------------------------

/**
 * Attempt to resolve every binding in an invariant against the current
 * filesystem. Returns one BindingResolution per binding. The invariant
 * itself is "decayed" if any binding decays; "holds" only if every
 * binding resolves cleanly.
 *
 * In v1 we check:
 *   1. binding.node.filePath exists in the project
 *   2. the file's line count covers binding.node.startLine and endLine
 *   3. (when nodeHash is non-empty) the recorded line range's content
 *      hashes to the recorded value
 *
 * Step 3+ will replace the line-range check with real AST node lookup
 * via the substrate; at that point a rename or extraction is detected
 * separately from a content edit.
 */
export function resolveBindings(
  invariant: StoredInvariant,
  projectRoot: string,
): BindingResolution[] {
  const out: BindingResolution[] = [];
  for (const b of invariant.bindings) {
    const absPath = resolveAbs(projectRoot, b.node.filePath);

    if (!existsSync(absPath)) {
      out.push({
        smt_constant: b.smt_constant,
        status: "decayed",
        decayKind: "deleted",
        reason: `file not found: ${b.node.filePath}`,
      });
      continue;
    }

    let content: string;
    try {
      content = readFileSync(absPath, "utf-8");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      out.push({
        smt_constant: b.smt_constant,
        status: "decayed",
        decayKind: "changed",
        reason: `read failed: ${msg}`,
      });
      continue;
    }

    const lines = content.split("\n");
    if (b.node.startLine > lines.length || b.node.endLine > lines.length) {
      out.push({
        smt_constant: b.smt_constant,
        status: "decayed",
        decayKind: "changed",
        reason:
          `line range ${b.node.startLine}-${b.node.endLine} exceeds file length ${lines.length}`,
      });
      continue;
    }

    // When we have a recorded node hash, verify the line range still
    // hashes to it. Empty nodeHash = v1 invariant written before the
    // substrate writer existed; skip the hash check (file existence +
    // line-range bound is the best we can do).
    if (b.node.nodeHash) {
      const span = lines.slice(b.node.startLine - 1, b.node.endLine).join("\n");
      const currentHash = sha256Prefix16(span);
      if (currentHash !== b.node.nodeHash) {
        out.push({
          smt_constant: b.smt_constant,
          status: "decayed",
          decayKind: "changed",
          reason: `node hash mismatch (was ${b.node.nodeHash}, now ${currentHash})`,
        });
        continue;
      }
    }

    out.push({ smt_constant: b.smt_constant, status: "resolved" });
  }
  return out;
}

function sha256Prefix16(s: string): string {
  // Local sha256 to avoid a circular dep on invariantStore. Same algo
  // (sha256, hex, slice 16) the writer uses.
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { createHash } = require("crypto");
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function resolveAbs(projectRoot: string, p: string): string {
  if (p.startsWith("/")) return p;
  return `${projectRoot}/${p}`;
}

// ---------------------------------------------------------------------------
// Top-level verify
// ---------------------------------------------------------------------------

/**
 * Load every (non-retired) invariant and resolve bindings. Returns a
 * structured report the CLI can format for humans or pipe to JSON.
 *
 * v1 verdict semantics: an invariant "holds" if all its bindings
 * resolve. It's "decayed" if any binding decays. "Violated" remains
 * impossible until step 5 wires Z3 path-checking into this function.
 */
export function verifyAll(projectRoot: string): VerifyReport {
  const invariants = readInvariants(projectRoot);
  const verdicts: InvariantVerdict[] = invariants.map((inv) => {
    const bindings = resolveBindings(inv, projectRoot);
    const anyDecayed = bindings.some((b) => b.status === "decayed");
    return {
      invariant: inv,
      status: anyDecayed ? "decayed" : "holds",
      bindings,
      pathCheck: "skipped",
    };
  });

  const summary = {
    total: verdicts.length,
    holds: verdicts.filter((v) => v.status === "holds").length,
    decayed: verdicts.filter((v) => v.status === "decayed").length,
    violated: 0,
  };

  return { verdicts, summary };
}

// ---------------------------------------------------------------------------
// Human-readable formatting
// ---------------------------------------------------------------------------

export function formatReport(report: VerifyReport, options: { verbose?: boolean } = {}): string {
  const lines: string[] = [];
  const { summary, verdicts } = report;

  lines.push(`provekit verify: ${summary.total} invariant${summary.total === 1 ? "" : "s"}`);

  if (summary.total === 0) {
    lines.push(`  (no invariants in .provekit/invariants/)`);
    lines.push(`  the standing runtime activates as soon as the fix loop ships its first bundle.`);
    return lines.join("\n");
  }

  if (summary.holds > 0) {
    const ids = verdicts.filter((v) => v.status === "holds").map((v) => v.invariant.id);
    lines.push(`  holds (${summary.holds}): ${options.verbose ? ids.join(", ") : `${summary.holds} invariant${summary.holds === 1 ? "" : "s"} pass binding resolution`}`);
  }

  for (const v of verdicts.filter((x) => x.status === "decayed")) {
    lines.push(`  ⚠ decay: ${v.invariant.id} — ${v.invariant.originatingBug.slice(0, 80)}`);
    for (const b of v.bindings.filter((x) => x.status === "decayed")) {
      lines.push(`      binding ${b.smt_constant} (${b.decayKind}): ${b.reason}`);
    }
    lines.push(
      `      remediation: re-run \`provekit fix\` against this locus, or retire with \`provekit invariants retire ${v.invariant.id}\``,
    );
  }

  // v1 doesn't surface violations because step 5 hasn't shipped. Once
  // it does, format violations here with the Z3 witness.

  return lines.join("\n");
}

/**
 * Map a verify report to the spec's exit codes:
 *   0 = all hold or undecidable
 *   1 = at least one violation (impossible in v1, reserved for step 5)
 *   2 = at least one decay, no violations
 *   3 = internal error (handled by the caller)
 */
export function exitCodeFor(report: VerifyReport): number {
  if (report.summary.violated > 0) return 1;
  if (report.summary.decayed > 0) return 2;
  return 0;
}
