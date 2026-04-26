/**
 * Phase 2-B: discovery mode.
 *
 * For HarvestCandidates that recognition mode could not match against the
 * existing library, ask the LLM to distill a principle from the diff +
 * commit message. Wraps the production `tryExistingCapabilities` (C6's
 * principle-distillation primitive) with synthesized inputs.
 *
 * Cost: ~1 LLM call for InvariantClaim synthesis + ~1-3 calls inside C6
 * (proposer + adversarial + optional refinement). Wall time per candidate
 * is comparable to a single fix-loop run on Bug-1.
 *
 * The discovery output is a `DiscoveryResult` carrying either the
 * principle(s) C6 produced or a structured "why nothing came out" reason.
 * The caller decides what to do with it (write to staging, promote to
 * library, defer).
 */

import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { dirname, join } from "path";
import { tmpdir } from "os";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { fileURLToPath } from "url";
import { openDb } from "../../db/index.js";
import { buildSASTForFile } from "../../sast/builder.js";
import { tryExistingCapabilities } from "../principleGen.js";
import { requestStructuredJson } from "../llm/structuredOutput.js";
import type { LLMProvider, InvariantClaim, PrincipleCandidate } from "../types.js";
import type { HarvestCandidate } from "./extractBugs.js";
import {
  synthesizeBugSignal,
  synthesizeFixCandidate,
  buildInvariantSynthesisPrompt,
} from "./synthesize.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface DiscoveryResult {
  /** Principles produced by C6 (typically 1-3 alternative shapes per bug class). */
  principles: PrincipleCandidate[];
  /** "ok" when at least one principle survived. Otherwise the failure reason. */
  outcome:
    | { kind: "ok"; principleCount: number }
    | { kind: "invariant_synthesis_failed"; reason: string }
    | { kind: "non_codifiable" }
    | { kind: "all_shapes_rejected"; rejectedShapes: { name: string; evidence: string }[] }
    | { kind: "capability_gap"; gap: string }
    | { kind: "error"; reason: string };
  /** The synthesized invariant the LLM produced (null if synthesis failed). */
  synthesizedInvariant: InvariantClaim | null;
  /** Raw LLM response for the invariant synthesis call (for inspection/debugging). */
  invariantRaw?: unknown;
}

export interface DiscoveryOptions {
  candidate: HarvestCandidate;
  llm: LLMProvider;
  /** Optional parent for the scratch dir; tests scope this to per-test parents. */
  scratchParent?: string;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function discoverPrinciple(opts: DiscoveryOptions): Promise<DiscoveryResult> {
  const { candidate, llm } = opts;
  const scratchParent = opts.scratchParent ?? tmpdir();
  const scratchDir = mkdtempSync(join(scratchParent, "provekit-harvest-discover-"));

  let db: ReturnType<typeof openDb> | null = null;
  try {
    // 1. Synthesize BugSignal + FixCandidate (mechanical, no LLM).
    const signal = synthesizeBugSignal(candidate);
    const fixCandidate = synthesizeFixCandidate(candidate);

    // 2. LLM call: synthesize InvariantClaim from commit message + diff.
    let invariantRaw: unknown;
    let synthesizedInvariant: InvariantClaim | null = null;
    try {
      invariantRaw = await requestStructuredJson<unknown>({
        prompt: buildInvariantSynthesisPrompt(candidate),
        llm,
        stage: "harvest-invariant-synthesis",
        // Sonnet is plenty for distilling an invariant from a diff; the
        // production C1 path uses opus, but harvest is bulk-mode.
        model: "sonnet",
      });
      synthesizedInvariant = parseInvariantClaim(invariantRaw);
    } catch (err) {
      return {
        principles: [],
        outcome: {
          kind: "invariant_synthesis_failed",
          reason: err instanceof Error ? err.message : String(err),
        },
        synthesizedInvariant: null,
      };
    }

    if (!synthesizedInvariant) {
      return {
        principles: [],
        outcome: {
          kind: "invariant_synthesis_failed",
          reason: "LLM response missing required InvariantClaim fields",
        },
        synthesizedInvariant: null,
        invariantRaw,
      };
    }

    // 3. Materialize buggy production files into the scratch tree.
    for (const [relPath, content] of Object.entries(candidate.buggyFiles)) {
      const abs = join(scratchDir, "src", relPath);
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content, "utf-8");
    }

    // 4. Open scratch DB + migrations + SAST build.
    const dbPath = join(scratchDir, "scratch.db");
    db = openDb(dbPath);
    migrate(db, { migrationsFolder: resolveMigrationsDir() });

    for (const relPath of Object.keys(candidate.buggyFiles)) {
      try {
        buildSASTForFile(db, join(scratchDir, "src", relPath));
      } catch {
        // Per-file build errors are non-fatal — we may still get matches
        // from files that did build.
      }
    }

    // 5. Call C6's principle-distillation primitive.
    const attempt = await tryExistingCapabilities({
      signal,
      invariant: synthesizedInvariant,
      fixCandidate,
      db,
      llm,
    });

    if (attempt.kind === "ok") {
      return {
        principles: attempt.principles,
        outcome: { kind: "ok", principleCount: attempt.principles.length },
        synthesizedInvariant,
        invariantRaw,
      };
    }
    if (attempt.kind === "non_codifiable") {
      return {
        principles: [],
        outcome: { kind: "non_codifiable" },
        synthesizedInvariant,
        invariantRaw,
      };
    }
    if (attempt.kind === "all_shapes_rejected") {
      return {
        principles: [],
        outcome: { kind: "all_shapes_rejected", rejectedShapes: attempt.rejectedShapes },
        synthesizedInvariant,
        invariantRaw,
      };
    }
    // capability_gap
    return {
      principles: [],
      outcome: { kind: "capability_gap", gap: attempt.gap },
      synthesizedInvariant,
      invariantRaw,
    };
  } catch (err) {
    return {
      principles: [],
      outcome: { kind: "error", reason: err instanceof Error ? err.message : String(err) },
      synthesizedInvariant: null,
    };
  } finally {
    try { if (db) db.$client.close(); } catch { /* ignore */ }
    try { rmSync(scratchDir, { recursive: true, force: true }); } catch { /* ignore */ }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function parseInvariantClaim(raw: unknown): InvariantClaim | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;

  const description = typeof obj["description"] === "string" ? obj["description"] : null;
  const violation = typeof obj["smt_violation_assertion"] === "string"
    ? obj["smt_violation_assertion"]
    : null;
  if (!description || !violation) return null;

  // Combine declarations + assertion into formalExpression (the production
  // shape downstream C-stages expect).
  const decls = Array.isArray(obj["smt_declarations"]) ? (obj["smt_declarations"] as unknown[]) : [];
  const declStr = decls.filter((d) => typeof d === "string").join("\n");
  const formalExpression = declStr.length > 0 ? `${declStr}\n${violation}\n(check-sat)` : `${violation}\n(check-sat)`;

  const bindings: InvariantClaim["bindings"] = [];
  if (Array.isArray(obj["bindings"])) {
    for (const b of obj["bindings"] as unknown[]) {
      if (b && typeof b === "object") {
        const bb = b as Record<string, unknown>;
        const smt = typeof bb["smt_constant"] === "string" ? bb["smt_constant"] : null;
        const src = typeof bb["source_expr"] === "string" ? bb["source_expr"] : null;
        const sort = typeof bb["sort"] === "string" ? bb["sort"] : "Int";
        if (smt && src) bindings.push({ smt_constant: smt, source_expr: src, sort: sort as "Int" | "Bool" | "String" });
      }
    }
  }

  const llmKind = typeof obj["kind"] === "string" ? obj["kind"] : undefined;

  return {
    principleId: null, // novel from harvest's perspective
    description,
    formalExpression,
    bindings,
    complexity: 1,
    witness: null,
    citations: null,
    source: "llm",
    llmKind,
  };
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

function resolveMigrationsDir(): string {
  return join(__dirname, "..", "..", "..", "drizzle");
}
