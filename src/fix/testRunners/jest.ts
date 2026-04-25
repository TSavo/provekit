/**
 * Jest test runner adapter.
 * Self-registers at module load.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { registerTestRunner } from "./registry.js";

export function registerJest(): void {
  registerTestRunner({
    name: "jest",
    description: "Jest test runner",
    detect: (projectRoot) => {
      // 1.0 — jest.config.{ts,js,json,mjs} or jest.setup.{ts,js} exists
      const configFiles = [
        "jest.config.ts",
        "jest.config.js",
        "jest.config.json",
        "jest.config.mjs",
        "jest.setup.ts",
        "jest.setup.js",
      ];
      for (const cfg of configFiles) {
        if (existsSync(join(projectRoot, cfg))) {
          return 1.0;
        }
      }

      // Check package.json for jest presence
      const pkgPath = join(projectRoot, "package.json");
      if (existsSync(pkgPath)) {
        let pkg: Record<string, unknown>;
        try {
          pkg = JSON.parse(readFileSync(pkgPath, "utf8")) as Record<string, unknown>;
        } catch {
          return 0;
        }

        // 0.9 — devDependencies or dependencies includes "jest"
        const deps = (pkg["dependencies"] ?? {}) as Record<string, unknown>;
        const devDeps = (pkg["devDependencies"] ?? {}) as Record<string, unknown>;
        if ("jest" in deps || "jest" in devDeps) {
          return 0.9;
        }

        // 0.7 — scripts.test contains "jest"
        const scripts = (pkg["scripts"] ?? {}) as Record<string, unknown>;
        const testScript = scripts["test"];
        if (typeof testScript === "string" && testScript.includes("jest")) {
          return 0.7;
        }
      }

      return 0;
    },
    resolveRunnerBinary: (projectRoot) => join(projectRoot, "node_modules", ".bin", "jest"),
    invocation: (testFile) => [testFile, "--no-coverage"],
    parseOutcome: (exitCode, stdout, _stderr) => ({
      passed: exitCode === 0,
      testCount: parseInt(stdout.match(/(\d+)\s+passed/)?.[1] ?? "0", 10),
      details: stdout.slice(-2000),
    }),
  });
}

registerJest();
