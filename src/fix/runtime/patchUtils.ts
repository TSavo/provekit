/**
 * Shared helpers for reading C3's patch output.
 *
 * Multiple downstream stages need to ground on "the file C3 actually
 * patched" rather than B-stage Locate's best-guess (`locus.file`). C5
 * uses this to pick the test target; the orchestrator's invariant
 * persistence path uses it to populate StoredInvariant.callsite +
 * binding nodes. Keeping the picker in one place avoids the two-flavor
 * bug where each consumer reimplements it slightly differently.
 */

import type { FixCandidate, CodePatchFileEdit } from "../types.js";

/**
 * Return the relative path of the file C3 patched (largest edit if multi-
 * file). The returned path is relative to the overlay worktree root —
 * matches the shape Locate uses, so it's a drop-in replacement for
 * locus.file. Returns null if the patch list is empty.
 */
export function pickPrimaryPatchFile(fix: FixCandidate): string | null {
  const edit = pickPrimaryPatchEdit(fix);
  return edit ? edit.file : null;
}

/**
 * Return the full CodePatchFileEdit (with `newContent`) for the primary
 * patched file. Caller can read `newContent` to compute post-edit line
 * geometry without touching disk or the overlay.
 */
export function pickPrimaryPatchEdit(
  fix: FixCandidate,
): CodePatchFileEdit | null {
  const edits = fix.patch?.fileEdits ?? [];
  if (edits.length === 0) return null;
  if (edits.length === 1) return edits[0];
  // Multi-file: pick the edit with the longest newContent (most substantive).
  let primary = edits[0];
  for (const e of edits) {
    if ((e.newContent?.length ?? 0) > (primary.newContent?.length ?? 0)) {
      primary = e;
    }
  }
  return primary;
}

/**
 * Convert a 0-indexed character offset within `text` to a 1-indexed line
 * number. Uses the count of newline chars preceding `offset`. Returns 1
 * for offset 0.
 */
export function offsetToLine(text: string, offset: number): number {
  if (offset <= 0) return 1;
  let line = 1;
  const limit = Math.min(offset, text.length);
  for (let i = 0; i < limit; i++) {
    if (text.charCodeAt(i) === 10 /* \n */) line++;
  }
  return line;
}

/**
 * Locate `needle` as a literal substring in `haystack` and return its
 * 1-indexed line range. Returns null if not found.
 *
 * Trims the needle (the LLM-emitted `source_expr` often has stray
 * whitespace) and computes endLine by counting newlines inside the
 * matched span — handles multi-line expressions.
 */
export function findExpressionLines(
  haystack: string,
  needle: string,
): { startLine: number; endLine: number } | null {
  const trimmed = needle.trim();
  if (trimmed.length === 0) return null;
  const idx = haystack.indexOf(trimmed);
  if (idx < 0) return null;
  const startLine = offsetToLine(haystack, idx);
  // Count newlines inside the matched span to compute endLine.
  let extra = 0;
  for (let i = idx; i < idx + trimmed.length; i++) {
    if (haystack.charCodeAt(i) === 10 /* \n */) extra++;
  }
  return { startLine, endLine: startLine + extra };
}

/**
 * Best-effort line range for a function declaration / arrow / method by
 * name in the post-edit file. Tries common JS/TS shapes:
 *   - `function NAME(`
 *   - `NAME =` (assignment / arrow)
 *   - `NAME(`  (method shorthand)
 * Returns the matched line as both start and end. Multi-line spans are
 * not computed here because v1 of the verify CLI only needs a starting
 * anchor for path enumeration.
 *
 * Returns null if no shape matches. Caller logs the miss.
 */
export function findFunctionLine(
  text: string,
  fnName: string | null | undefined,
): number | null {
  if (!fnName) return null;
  // Escape regex metachars in the name (function names can include $ and _).
  const esc = fnName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const patterns = [
    new RegExp(`\\bfunction\\s+${esc}\\s*[<(]`),
    new RegExp(`\\b(?:const|let|var)\\s+${esc}\\s*[:=]`),
    new RegExp(`(?:^|\\n)\\s*(?:async\\s+)?${esc}\\s*\\(`), // method shorthand
    new RegExp(`\\b${esc}\\s*=\\s*(?:async\\s*)?\\(`), // arrow assignment without let/const
  ];
  for (const re of patterns) {
    const m = re.exec(text);
    if (m) return offsetToLine(text, m.index);
  }
  return null;
}
