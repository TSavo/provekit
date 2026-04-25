/**
 * B3 mechanical-mode template instantiation helpers (C3m + C5m).
 *
 * Both fixTemplate and testTemplate are whole-file source strings with
 * `{{name}}` placeholders. Substitution is purely textual: identical
 * mechanism to the existing smt2Template substitution in C1m. The placeholders
 * are bound from:
 *
 *   1. Locus-derived bindings (the principle's match captures, mapped to
 *      identifier text via the SAST nodes table).
 *   2. Witness-derived bindings (for testTemplate only; populated from the
 *      Z3 witness model).
 *
 * v1 MVP: single-file fixes only. Multi-file mechanical fixes are out of
 * scope; novel/complex changes still go through the LLM-driven C3 path.
 */

import { readFileSync, existsSync } from "fs";
import { join, relative, dirname } from "path";
import { eq } from "drizzle-orm";
import type {
  BugLocus,
  CodePatch,
  FixTemplate,
  OverlayHandle,
  TestTemplate,
} from "../types.js";
import { nodes, files as filesTable } from "../../sast/schema/index.js";

// ---------------------------------------------------------------------------
// Locus → file path
// ---------------------------------------------------------------------------

/**
 * Resolve the relative file path of `locus.file` inside the overlay worktree.
 * Falls back to a basename under `src/` when the locus file lives outside the
 * worktree (mirroring chooseTestFilePath's logic).
 */
function resolveLocusFileRelative(locus: BugLocus, overlay: OverlayHandle): string {
  let rel: string;
  try {
    rel = relative(overlay.worktreePath, locus.file);
    if (rel.startsWith("..")) {
      // Outside the worktree — use bare name under src/
      const base = locus.file.split("/").pop() ?? "fixture.ts";
      rel = `src/${base}`;
    }
  } catch {
    const base = locus.file.split("/").pop() ?? "fixture.ts";
    rel = `src/${base}`;
  }
  return rel;
}

// ---------------------------------------------------------------------------
// Substitution
// ---------------------------------------------------------------------------

/** Replace every `{{name}}` in `source` using the substitution map. */
export function substitutePlaceholders(
  source: string,
  substitutions: Record<string, string>,
): string {
  let result = source;
  for (const [name, value] of Object.entries(substitutions)) {
    result = result.replaceAll(`{{${name}}}`, value);
  }
  return result;
}

/** Extract every `{{name}}` placeholder name from the source, deduplicated. */
export function extractTemplatePlaceholders(source: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const m of source.matchAll(/\{\{([^}]+)\}\}/g)) {
    const name = m[1]!.trim();
    if (!seen.has(name)) {
      seen.add(name);
      out.push(name);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// Bindings → substitution values
// ---------------------------------------------------------------------------

import type { Db } from "../../db/index.js";

/**
 * For each capture binding (capture name → SAST node id), look up the source
 * text of that node by reading the file slice [sourceStart, sourceEnd). Values
 * default to the capture name itself when the slice is unresolvable.
 *
 * The nodes table only stores byte offsets, not the raw text — so we read the
 * file via the joined `files.path` column.
 */
export function bindingsToSubstitutions(
  db: Db,
  bindings: Record<string, string>,
): Record<string, string> {
  const out: Record<string, string> = {};
  // Cache: fileId → file contents.
  const fileCache = new Map<number, string>();

  for (const [captureName, nodeId] of Object.entries(bindings)) {
    const row = db
      .select({
        sourceStart: nodes.sourceStart,
        sourceEnd: nodes.sourceEnd,
        fileId: nodes.fileId,
        path: filesTable.path,
      })
      .from(nodes)
      .innerJoin(filesTable, eq(filesTable.id, nodes.fileId))
      .where(eq(nodes.id, nodeId))
      .get();

    if (!row) {
      out[captureName] = captureName;
      continue;
    }

    let contents = fileCache.get(row.fileId);
    if (contents === undefined) {
      try {
        contents = readFileSync(row.path, "utf-8");
        fileCache.set(row.fileId, contents);
      } catch {
        out[captureName] = captureName;
        continue;
      }
    }

    out[captureName] = contents.slice(row.sourceStart, row.sourceEnd);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Public: instantiate fix template → CodePatch
// ---------------------------------------------------------------------------

export interface InstantiateFixTemplateArgs {
  template: FixTemplate;
  locus: BugLocus;
  overlay: OverlayHandle;
  bindings: Record<string, string>;
}

/**
 * Instantiate `template.pattern` as the new whole-file content for the locus
 * file. Captures bound by their match-node `text` value (extracted via the
 * SAST nodes table). Returns a single-fileEdit CodePatch.
 *
 * Special placeholders:
 *   {{__originalSource__}} — the current contents of the locus file in the
 *   overlay (useful when the fix is "wrap existing function" rather than
 *   replace it wholesale).
 */
export function instantiateFixTemplate(args: InstantiateFixTemplateArgs): CodePatch {
  const { template, locus, overlay, bindings } = args;
  const locusRel = resolveLocusFileRelative(locus, overlay);

  // Resolve {{__originalSource__}} if referenced.
  const substitutions: Record<string, string> = bindingsToSubstitutions(
    overlay.sastDb,
    bindings,
  );
  if (template.pattern.includes("{{__originalSource__}}")) {
    const overlayLocusPath = join(overlay.worktreePath, locusRel);
    let original = "";
    if (existsSync(overlayLocusPath)) {
      original = readFileSync(overlayLocusPath, "utf-8");
    }
    substitutions["__originalSource__"] = original;
  }

  const newContent = substitutePlaceholders(template.pattern, substitutions);

  return {
    fileEdits: [{ file: locusRel, newContent }],
    description: template.rationale,
  };
}

// ---------------------------------------------------------------------------
// Public: instantiate test template → { testFilePath, testCode }
// ---------------------------------------------------------------------------

export interface InstantiateTestTemplateArgs {
  template: TestTemplate;
  locus: BugLocus;
  overlay: OverlayHandle;
  bindings: Record<string, string>;
  /** Witness inputs from the C1m InvariantClaim. Substituted as JSON. */
  witnessInputs: Record<string, unknown>;
}

export interface InstantiatedTest {
  testFilePath: string;
  testCode: string;
}

/**
 * Instantiate `template.source` into a vitest test file. Returns the chosen
 * relative path (under the overlay worktree) and the rendered source text.
 *
 * Auto-resolved placeholders:
 *   {{importsFrom}}    — relative path from the test file to the locus module
 *                        (extension stripped). Useful for `import { X } from "{{importsFrom}}"`.
 *   {{witnessJson}}    — JSON.stringify(witnessInputs).
 *   {{<bindingName>}}  — node-text from each captured node, same as fix template.
 */
export function instantiateTestTemplate(args: InstantiateTestTemplateArgs): InstantiatedTest {
  const { template, locus, overlay, bindings, witnessInputs } = args;

  const locusRel = resolveLocusFileRelative(locus, overlay);
  const withoutExt = locusRel.replace(/\.(ts|tsx|js|jsx)$/, "");
  const testFilePath = `${withoutExt}.regression.test.ts`;

  const testDir = dirname(testFilePath);
  let importsFrom = relative(testDir, withoutExt);
  if (!importsFrom.startsWith(".")) importsFrom = `./${importsFrom}`;

  const substitutions: Record<string, string> = bindingsToSubstitutions(
    overlay.sastDb,
    bindings,
  );
  substitutions["importsFrom"] = importsFrom;
  substitutions["witnessJson"] = JSON.stringify(
    witnessInputs,
    (_k, v) => (typeof v === "bigint" ? v.toString() : v),
  );

  // Materialize each witness input as its own placeholder for simple subst.
  for (const [k, v] of Object.entries(witnessInputs)) {
    substitutions[`witness.${k}`] = String(v);
  }

  const testCode = substitutePlaceholders(template.source, substitutions);
  return { testFilePath, testCode };
}
