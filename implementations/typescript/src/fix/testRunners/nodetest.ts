/**
 * Node.js built-in test runner (node:test) adapter.
 * Self-registers at module load.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { registerTestRunner } from "./registry.js";

export function registerNodeTest(): void {
  registerTestRunner({
    name: "node-test",
    description: "Node.js built-in test runner (node:test)",
    detect: (projectRoot) => {
      // Check package.json scripts for "node --test"
      const pkgPath = join(projectRoot, "package.json");
      if (existsSync(pkgPath)) {
        let pkg: Record<string, unknown>;
        try {
          pkg = JSON.parse(readFileSync(pkgPath, "utf8")) as Record<string, unknown>;
        } catch {
          return 0;
        }

        const scripts = (pkg["scripts"] ?? {}) as Record<string, unknown>;
        for (const script of Object.values(scripts)) {
          if (typeof script === "string" && script.includes("node --test")) {
            return 0.9;
          }
        }
      }

      // 0.7 — any *.test.{js,ts} file in src/ contains `from "node:test"` or `require("node:test")`
      // We spot-check a few common file paths rather than globbing (fast heuristic)
      const candidateDirs = ["src", "test", "tests", "."];
      for (const dir of candidateDirs) {
        const dirPath = join(projectRoot, dir);
        if (!existsSync(dirPath)) continue;
        // Check a sentinel: package.json-declared "type" + presence of node:test usage
        // in the root package. Fast heuristic: if no config file but package.json mentions
        // "node:test" anywhere in its scripts, score 0.7.
        const pkgPathInner = join(projectRoot, "package.json");
        if (existsSync(pkgPathInner)) {
          const raw = readFileSync(pkgPathInner, "utf8");
          if (raw.includes("node:test")) {
            return 0.7;
          }
        }
        break;
      }

      return 0;
    },
    resolveRunnerBinary: (_projectRoot) => process.execPath,
    invocation: (testFile) => ["--test", testFile],
    parseOutcome: (exitCode, stdout, _stderr) => ({
      passed: exitCode === 0,
      // node:test stdout: "# tests N" or "ok N -" (TAP-like)
      testCount: parseInt(stdout.match(/# tests (\d+)/)?.[1] ?? "0", 10),
      details: stdout.slice(-2000),
    }),
  });
}

registerNodeTest();
