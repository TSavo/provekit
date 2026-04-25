/**
 * Mocha test runner adapter.
 * Self-registers at module load.
 */

import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { registerTestRunner } from "./registry.js";

export function registerMocha(): void {
  registerTestRunner({
    name: "mocha",
    description: "Mocha test runner",
    detect: (projectRoot) => {
      // 1.0 — .mocharc.{json,yml,js} or mocha.opts exists
      const configFiles = [
        ".mocharc.json",
        ".mocharc.yml",
        ".mocharc.yaml",
        ".mocharc.js",
        "mocha.opts",
      ];
      for (const cfg of configFiles) {
        if (existsSync(join(projectRoot, cfg))) {
          return 1.0;
        }
      }

      // Check package.json for mocha presence
      const pkgPath = join(projectRoot, "package.json");
      if (existsSync(pkgPath)) {
        let pkg: Record<string, unknown>;
        try {
          pkg = JSON.parse(readFileSync(pkgPath, "utf8")) as Record<string, unknown>;
        } catch {
          return 0;
        }

        // 0.9 — devDependencies or dependencies includes "mocha"
        const deps = (pkg["dependencies"] ?? {}) as Record<string, unknown>;
        const devDeps = (pkg["devDependencies"] ?? {}) as Record<string, unknown>;
        if ("mocha" in deps || "mocha" in devDeps) {
          return 0.9;
        }

        // 0.7 — scripts.test contains "mocha"
        const scripts = (pkg["scripts"] ?? {}) as Record<string, unknown>;
        const testScript = scripts["test"];
        if (typeof testScript === "string" && testScript.includes("mocha")) {
          return 0.7;
        }
      }

      return 0;
    },
    resolveRunnerBinary: (projectRoot) => join(projectRoot, "node_modules", ".bin", "mocha"),
    invocation: (testFile) => [testFile],
    parseOutcome: (exitCode, stdout, _stderr) => ({
      passed: exitCode === 0,
      // mocha stdout: "N passing"
      testCount: parseInt(stdout.match(/(\d+)\s+passing/)?.[1] ?? "0", 10),
      details: stdout.slice(-2000),
    }),
  });
}

registerMocha();
