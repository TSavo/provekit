/**
 * D1b: Artifact kind detection helpers.
 *
 * Utilities for inspecting a bundle's artifacts to determine which file paths
 * were touched and whether they match specific categories.
 */

import type { FixBundle } from "../types.js";

/** Files touched by the primary fix only. */
export function primaryFixFiles(artifacts: FixBundle["artifacts"]): string[] {
  if (!artifacts.primaryFix) return [];
  return artifacts.primaryFix.patch.fileEdits.map((e) => e.file);
}

/** Files touched by primaryFix OR any complementary change. */
export function touchedFiles(artifacts: FixBundle["artifacts"]): string[] {
  const files = new Set<string>();
  if (artifacts.primaryFix) {
    for (const edit of artifacts.primaryFix.patch.fileEdits) files.add(edit.file);
  }
  for (const comp of artifacts.complementary) {
    for (const edit of comp.patch.fileEdits) files.add(edit.file);
  }
  return [...files];
}

/** Returns true iff every file in the array satisfies predicate (and array is non-empty). */
export function touchesOnly(files: string[], predicate: (f: string) => boolean): boolean {
  return files.length > 0 && files.every(predicate);
}

/** Returns true iff at least one file satisfies predicate. */
export function touchesAny(files: string[], predicate: (f: string) => boolean): boolean {
  return files.some(predicate);
}

const base = (f: string): string => f.split("/").pop() ?? f;

export const isTestFile = (f: string): boolean =>
  /\.(test|spec)\.(ts|js|tsx|jsx)$/.test(f);

export const isConfigFile = (f: string): boolean =>
  /^(\.env(\..+)?|package\.json|tsconfig(\..+)?\.json|.+\.config\.(ts|js|mjs|cjs))$/.test(base(f)) ||
  f.startsWith("deploy/") ||
  f.startsWith("k8s/");

export const isDepFile = (f: string): boolean =>
  base(f) === "package.json" ||
  base(f) === "package-lock.json" ||
  base(f) === "pnpm-lock.yaml" ||
  base(f) === "yarn.lock";

export const isDocFile = (f: string): boolean =>
  /\.(md|mdx)$/.test(f) ||
  f.startsWith("docs/") ||
  f.startsWith("README");

export const isMigrationFile = (f: string): boolean =>
  f.startsWith("drizzle/") ||
  f.includes("/drizzle/") ||
  f.startsWith("migrations/") ||
  f.includes("/migrations/");

export const isPromptFile = (f: string): boolean =>
  f.includes("/prompts/") ||
  f.includes("systemPrompt");

export const isLintFile = (f: string): boolean =>
  /^(\.eslintrc(\..+)?|biome\.json|eslint\.config\.(ts|js|mjs|cjs)|.*\.lint\.ts)$/.test(base(f));
