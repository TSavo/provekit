/**
 * Cross-language equivalence regression test.
 *
 * Spawns the harness which builds and runs the TS / Rust / Go / C++ runners
 * for every fixture, asserts byte-identical compact JSON across all four
 * kits, and asserts SHA256 matches the locked golden in goldens.txt.
 *
 * If a kit drifts in a way that breaks cross-language equivalence (a
 * canonical-form change in one language but not another), this gate fails
 * before any consumer notices their proofHashes diverging.
 *
 * Skipped automatically when any toolchain is missing — the gate is a
 * positive signal when it runs, not a build blocker on environments that
 * don't have all four compilers.
 */

import { describe, it, expect } from "vitest";
import { execSync, spawnSync } from "child_process";
import { existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const HARNESS = join(__dirname, "..", "..", "..", "scripts", "cross-lang-equivalence", "harness.sh");

function hasCommand(cmd: string): boolean {
  try {
    execSync(`command -v ${cmd}`, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function hasCargo(): boolean {
  if (hasCommand("cargo")) return true;
  return existsSync(`${process.env.HOME}/.cargo/bin/cargo`);
}

const haveAllToolchains =
  hasCommand("npx") && hasCommand("go") && hasCommand("clang++") && hasCargo();

describe("cross-language IR equivalence", () => {
  it.skipIf(!haveAllToolchains)(
    "all kits (TS, Rust, Go, C++) emit byte-identical IR for every fixture",
    () => {
      const result = spawnSync("bash", [HARNESS], {
        encoding: "utf-8",
        timeout: 180_000,
      });
      if (result.status !== 0) {
        throw new Error(
          `harness failed (exit ${result.status})\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
        );
      }
      expect(result.stdout).toMatch(/passed, 0 failed/);
    },
    240_000,
  );
});
