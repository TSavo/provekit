/**
 * Vitest test runner adapter.
 * Self-registers at module load.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { registerTestRunner } from "./registry.js";

export function registerVitest(): void {
  registerTestRunner({
    name: "vitest",
    description: "Vitest test runner",
    detect: (projectRoot) => {
      // 1.0 — vitest.config.{ts,js,mjs} exists
      const configFiles = ["vitest.config.ts", "vitest.config.js", "vitest.config.mjs"];
      for (const cfg of configFiles) {
        if (existsSync(join(projectRoot, cfg))) {
          return 1.0;
        }
      }

      // Check package.json for vitest presence
      const pkgPath = join(projectRoot, "package.json");
      if (existsSync(pkgPath)) {
        let pkg: Record<string, unknown>;
        try {
          pkg = JSON.parse(readFileSync(pkgPath, "utf8")) as Record<string, unknown>;
        } catch {
          return 0;
        }

        // 0.9 — devDependencies or dependencies includes "vitest"
        const deps = (pkg["dependencies"] ?? {}) as Record<string, unknown>;
        const devDeps = (pkg["devDependencies"] ?? {}) as Record<string, unknown>;
        if ("vitest" in deps || "vitest" in devDeps) {
          return 0.9;
        }

        // 0.7 — scripts.test contains "vitest"
        const scripts = (pkg["scripts"] ?? {}) as Record<string, unknown>;
        const testScript = scripts["test"];
        if (typeof testScript === "string" && testScript.includes("vitest")) {
          return 0.7;
        }
      }

      return 0;
    },
    resolveRunnerBinary: (projectRoot) => join(projectRoot, "node_modules", ".bin", "vitest"),
    invocation: (testFile) => ["run", testFile, "--reporter=default"],
    parseOutcome: (exitCode, stdout, _stderr) => ({
      passed: exitCode === 0,
      testCount: parseInt(stdout.match(/Tests\s+(\d+)\s+passed/)?.[1] ?? "0", 10),
      details: stdout.slice(-2000),
    }),
  });
}

registerVitest();
