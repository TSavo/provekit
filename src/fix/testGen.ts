/**
 * C5: testGen helpers — witness extraction, test code generation, overlay execution, mutation.
 *
 * Vitest-in-overlay notes (empirically verified):
 *   - git worktrees inherit all checked-in files but NOT node_modules.
 *   - We create a node_modules symlink inside the overlay pointing at the main
 *     repo's node_modules before running vitest.
 *   - The overlay's checked-in vitest.config.ts has include: ["src/**\/**.test.ts"],
 *     which covers src/<name>.regression.test.ts.
 *   - We run the local vitest binary (overlay/node_modules/.bin/vitest) from
 *     overlay.worktreePath with the relative test path as the filter.
 *   - Exit code 0 = all tests pass; non-zero = one or more failures.
 */

import { readFileSync, writeFileSync, symlinkSync, existsSync } from "fs";
import { join, relative, dirname, basename, resolve } from "path";
import { spawnSync } from "child_process";
import { execFileSync } from "child_process";
import { parseZ3Model } from "../z3/modelParser.js";
import { materializeZ3Value } from "../inputs/synthesizer.js";
import { applyPatchToOverlay, reindexOverlay } from "./overlay.js";
import type { InvariantClaim, BugLocus, BugSignal, OverlayHandle, CodePatch, LLMProvider } from "./types.js";

// ---------------------------------------------------------------------------
// extractWitnessInputs
// ---------------------------------------------------------------------------

/**
 * Parse the Z3 witness from InvariantClaim into materialized JS values keyed
 * by source_expr (the source-code parameter name, e.g. "a", "b").
 *
 * Throws if invariant.witness is null (C1 must have produced one).
 * Returns {} if the witness parses to an empty model (soft case — LLM can
 * still generate a test using the locus source as context).
 */
export function extractWitnessInputs(invariant: InvariantClaim): Record<string, unknown> {
  if (invariant.witness === null || invariant.witness === undefined) {
    throw new Error(
      "C5: invariant has no Z3 witness — C1 must have failed to produce one. Cannot generate mutation-verified regression test.",
    );
  }

  const model = parseZ3Model(invariant.witness);
  const inputs: Record<string, unknown> = {};

  for (const binding of invariant.bindings) {
    const value = model.get(binding.smt_constant);
    if (value !== undefined) {
      inputs[binding.source_expr] = materializeZ3Value(value);
    }
  }

  return inputs;
}

// ---------------------------------------------------------------------------
// chooseTestFilePath
// ---------------------------------------------------------------------------

/**
 * Derive a relative test file path from locus.file.
 * Example: "src/utils/math.ts" → "src/utils/math.regression.test.ts"
 *
 * The path is relative to the overlay's worktreePath, consistent with CodePatch.
 * Since this lives inside the overlay's git worktree (which has vitest.config.ts
 * with include: ["src/**\/*.test.ts"]), placing it under src/ ensures vitest picks it up.
 */
export function chooseTestFilePath(locus: BugLocus, overlay: OverlayHandle): string {
  // locus.file is an absolute path. Convert to relative within the overlay worktree.
  const worktreeReal = overlay.worktreePath;
  const locusAbs = locus.file;

  // Try to compute relative path. If locus.file is outside the worktree (e.g. original repo),
  // fall back to using just the filename portion under src/.
  let rel: string;
  try {
    rel = relative(worktreeReal, locusAbs);
    // If the relative path escapes the worktree (starts with ..), use basename approach
    if (rel.startsWith("..")) {
      const base = basename(locusAbs, ".ts");
      rel = `src/${base}.ts`;
    }
  } catch {
    const base = basename(locusAbs, ".ts");
    rel = `src/${base}.ts`;
  }

  // Strip the extension and append .regression.test.ts
  const withoutExt = rel.replace(/\.(ts|tsx|js|jsx)$/, "");
  return `${withoutExt}.regression.test.ts`;
}

// ---------------------------------------------------------------------------
// generateTestCode
// ---------------------------------------------------------------------------

/**
 * Ask the LLM to produce a full vitest test file.
 *
 * The test should:
 * - Import the function from the module under test (relative path from the test file)
 * - Call the function with the Z3 witness inputs
 * - Assert the invariant holds (no throw, or return value satisfies the predicate)
 *
 * Returns the full test file source code.
 * Throws if the LLM response is unparseable or clearly broken.
 */
export async function generateTestCode(args: {
  signal: BugSignal;
  locus: BugLocus;
  invariant: InvariantClaim;
  inputs: Record<string, unknown>;
  testFilePath: string;
  testName: string;
  llm: LLMProvider;
  overlay: OverlayHandle;
}): Promise<string> {
  const { signal, locus, invariant, inputs, testFilePath, testName, llm, overlay } = args;

  // Read the locus file source from the overlay (it's already been patched by C3)
  let functionSource = "(source unavailable)";
  try {
    const locusRel = relative(overlay.worktreePath, locus.file);
    const overlayLocusPath = join(overlay.worktreePath, locusRel);
    if (existsSync(overlayLocusPath)) {
      functionSource = readFileSync(overlayLocusPath, "utf8");
    } else {
      // Fallback: try locus.file directly (may be the original repo path)
      if (existsSync(locus.file)) {
        functionSource = readFileSync(locus.file, "utf8");
      }
    }
  } catch {
    // Non-fatal — LLM can still generate based on signal + invariant alone
  }

  // Derive the relative import path from the test file to the module under test
  const testDir = dirname(testFilePath);
  const locusRelToWorktree = (() => {
    try {
      const r = relative(overlay.worktreePath, locus.file);
      return r.startsWith("..") ? null : r;
    } catch {
      return null;
    }
  })();

  let importPath = "./fixture";
  if (locusRelToWorktree) {
    const rel = relative(testDir, locusRelToWorktree.replace(/\.(ts|tsx)$/, ""));
    importPath = rel.startsWith(".") ? rel : `./${rel}`;
  }

  const inputsJson = JSON.stringify(inputs, (_k, v) =>
    typeof v === "bigint" ? v.toString() : v
  , 2);

  const prompt = `You are a TypeScript testing expert. Generate a complete vitest regression test file.

TASK: Generate a vitest test that:
1. Imports the buggy function from the module under test
2. Calls the function with the exact Z3 witness inputs provided
3. Asserts the invariant holds (the function should NOT exhibit the bug)

INVARIANT VIOLATED: ${invariant.description}

BUG SUMMARY: ${signal.summary}

Z3 WITNESS INPUTS (values that trigger the bug before the fix):
${inputsJson}

MODULE UNDER TEST (locus file): ${locus.file}
FUNCTION/LINE: ${locus.function ?? "(unknown)"} at line ${locus.line}

SOURCE CODE (post-fix, in overlay):
\`\`\`typescript
${functionSource.slice(0, 2000)}
\`\`\`

IMPORT PATH (from test file to module): ${importPath}

TEST FILE PATH: ${testFilePath}
TEST NAME: ${testName}

REQUIREMENTS:
- Use vitest (import { it, expect, describe } from "vitest")
- Import the function by name from the module (named import preferred; if unclear use default import)
- Call the function with the exact Z3 witness input values
- Assert the function either: (a) does not throw, OR (b) returns a value that satisfies the invariant
- If the function takes b=0 as a denominator, assert it does NOT throw (e.g. wraps in expect(...).not.toThrow())
- Use the test name exactly: "${testName}"
- Output ONLY the complete TypeScript test file contents, no markdown, no explanation

Example structure:
import { it, expect } from "vitest";
import { divide } from "../utils/math";

it("${testName}", () => {
  // Z3 witness: a=5, b=0 triggers division-by-zero before fix
  expect(() => divide(5, 0)).not.toThrow();
});
`;

  const raw = await llm.complete({ prompt, model: "sonnet" });

  // Extract code block if LLM wraps in markdown fences
  let code = raw.trim();
  if (code.startsWith("```")) {
    code = code.replace(/^```(?:typescript|ts)?\n?/, "").replace(/\n?```\s*$/, "").trim();
  }

  // Validate: must have at least one `it(` call
  if (!code.includes("it(") && !code.includes("it.")) {
    throw new Error(
      `C5: LLM-generated test code has no it() call. Cannot use this as a regression test. Raw response: ${raw.slice(0, 300)}`,
    );
  }

  // Validate: must import from vitest
  if (!code.includes("vitest")) {
    throw new Error(
      `C5: LLM-generated test code does not import from vitest. Raw response: ${raw.slice(0, 300)}`,
    );
  }

  return code;
}

// ---------------------------------------------------------------------------
// setupOverlayForTest
// ---------------------------------------------------------------------------

/**
 * Ensure the overlay directory has a node_modules symlink pointing at the
 * main repo's node_modules. This is needed because git worktrees do NOT
 * inherit node_modules — they only inherit checked-in files.
 *
 * The symlink is created once; subsequent calls are no-ops.
 */
export function setupOverlayForTest(overlay: OverlayHandle, mainRepoRoot: string): void {
  const nmTarget = join(overlay.worktreePath, "node_modules");
  if (!existsSync(nmTarget)) {
    const nmSource = join(mainRepoRoot, "node_modules");
    if (!existsSync(nmSource)) {
      throw new Error(
        `C5: main repo node_modules not found at ${nmSource}. Cannot run vitest in overlay.`,
      );
    }
    symlinkSync(nmSource, nmTarget);
  }
}

// ---------------------------------------------------------------------------
// RunResult
// ---------------------------------------------------------------------------

export interface RunResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

// ---------------------------------------------------------------------------
// runTestInOverlay
// ---------------------------------------------------------------------------

/**
 * Execute vitest against a single test file inside the overlay.
 *
 * Strategy (empirically verified):
 *   1. The overlay inherits vitest.config.ts from HEAD (git worktree).
 *   2. The config has include: ["src/**\/*.test.ts"] — test file must live under src/.
 *   3. We symlink node_modules from the main repo into the overlay.
 *   4. Run the overlay's vitest binary with the relative test file path as filter.
 *   5. Exit code 0 = pass; non-zero = fail.
 *
 * Returns { exitCode, stdout, stderr }. Never throws — caller interprets exit code.
 */
export function runTestInOverlay(
  overlay: OverlayHandle,
  testFilePath: string,
  mainRepoRoot: string,
): RunResult {
  // Ensure node_modules symlink exists
  setupOverlayForTest(overlay, mainRepoRoot);

  const vitestBin = join(overlay.worktreePath, "node_modules", ".bin", "vitest");

  const result = spawnSync(
    vitestBin,
    ["run", "--reporter=verbose", testFilePath],
    {
      cwd: overlay.worktreePath,
      encoding: "utf8",
      timeout: 60000,
      env: { ...process.env, NODE_ENV: "test", CI: "true" },
    },
  );

  return {
    exitCode: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

// ---------------------------------------------------------------------------
// revertFixInOverlay
// ---------------------------------------------------------------------------

/**
 * Revert the fix in the overlay by restoring each patched file to its
 * HEAD (pre-fix) content via `git show HEAD:<file>`.
 *
 * Returns a Map<relPath, postFixContent> so the fix can be restored afterward.
 * Edge case: if a file was NEW in the patch (didn't exist at HEAD), we store
 * an empty sentinel and unlink the file for the revert.
 */
export function revertFixInOverlay(
  overlay: OverlayHandle,
  patch: CodePatch,
): Map<string, string> {
  const postFixContents = new Map<string, string>();

  for (const edit of patch.fileEdits) {
    const absPath = join(overlay.worktreePath, edit.file);

    // Stash the current (post-fix) content
    let currentContent: string;
    try {
      currentContent = readFileSync(absPath, "utf8");
    } catch {
      currentContent = "";
    }
    postFixContents.set(edit.file, currentContent);

    // Restore from git HEAD (pre-fix original)
    try {
      const originalContent = execFileSync(
        "git",
        ["show", `HEAD:${edit.file}`],
        { cwd: overlay.worktreePath, encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] },
      );
      writeFileSync(absPath, originalContent, "utf8");
    } catch {
      // File didn't exist at HEAD (new file added by the fix).
      // Delete it to simulate the pre-fix state.
      try {
        const { unlinkSync } = require("fs") as typeof import("fs");
        if (existsSync(absPath)) {
          unlinkSync(absPath);
        }
      } catch {
        // Best effort
      }
    }
  }

  return postFixContents;
}

// ---------------------------------------------------------------------------
// restoreFixInOverlay
// ---------------------------------------------------------------------------

/**
 * Restore the fix in the overlay after the mutation check.
 * Writes back the post-fix contents stashed by revertFixInOverlay.
 */
export function restoreFixInOverlay(
  overlay: OverlayHandle,
  postFixContents: Map<string, string>,
): void {
  for (const [relFile, content] of postFixContents) {
    const absPath = join(overlay.worktreePath, relFile);
    if (content === "") {
      // File was new-in-patch and was deleted during revert; restore it
      // by re-creating it (it may have been unlinked above)
    }
    writeFileSync(absPath, content, "utf8");
  }
}

// ---------------------------------------------------------------------------
// resolveMainRepoRoot
// ---------------------------------------------------------------------------

/**
 * Resolve the main repository root from an overlay handle.
 * The overlay worktree's .git file points at the common git dir — we use
 * `git rev-parse --show-toplevel` from inside the overlay to get the worktree
 * path, then find the common git dir, then the main worktree root.
 *
 * Simpler: the main repo root is the directory where we originally ran
 * `git worktree add`. We recover it by following the overlay's .git gitfile.
 */
export function resolveMainRepoRoot(overlay: OverlayHandle): string {
  try {
    // The overlay's .git is a file containing "gitdir: <commonGitDir>/worktrees/<name>"
    // We can ask git to give us the common dir.
    const commonDir = execFileSync(
      "git",
      ["rev-parse", "--git-common-dir"],
      { cwd: overlay.worktreePath, encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] },
    ).trim();

    // commonDir may be relative (e.g. "../../.git") or absolute.
    // Use path.resolve — unlike path.join it handles absolute second args correctly.
    const resolvedCommon = resolve(overlay.worktreePath, commonDir);
    return dirname(resolvedCommon);
  } catch {
    // Fallback: use process.cwd() — this works when C5 is called from the main repo
    return process.cwd();
  }
}
