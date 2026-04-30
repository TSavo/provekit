import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { execFileSync } from "child_process";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { ProofDiff } from "./ProofDiff";

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf-8" }).trim();
}

function writeContract(
  projectRoot: string,
  contract: {
    key: string;
    file: string;
    function: string;
    line: number;
    proven: { principle: string | null; principle_hash: string; claim: string; smt2: string }[];
    violations: { principle: string | null; principle_hash: string; claim: string; smt2: string }[];
  },
): void {
  const dir = join(projectRoot, ".provekit", "contracts");
  mkdirSync(dir, { recursive: true });
  const full = {
    ...contract,
    signal_hash: "h",
    clause_history: [],
    depends_on: [],
  };
  // Bundle as `{ contracts: [...] }` so both ContractStore (current disk read)
  // and ProofDiff.loadContractsAtRef (git-show parse, only checks data.contracts)
  // see the same data.
  const fileName = contract.key.replace(/[^a-zA-Z0-9_-]/g, "_") + ".json";
  writeFileSync(join(dir, fileName), JSON.stringify({ contracts: [full] }, null, 2));
}

describe("ProofDiff", () => {
  let projectRoot: string;
  let logSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    projectRoot = mkdtempSync(join(tmpdir(), "provekit-proofdiff-"));
    logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    logSpy.mockRestore();
    rmSync(projectRoot, { recursive: true, force: true });
  });

  it("returns no changes when previous and current contracts match", () => {
    git(projectRoot, "init", "-q");
    git(projectRoot, "config", "user.email", "t@e.com");
    git(projectRoot, "config", "user.name", "T");

    writeContract(projectRoot, {
      key: "src/a.ts/foo[10]",
      file: "src/a.ts",
      function: "foo",
      line: 10,
      proven: [{ principle: "non-zero", principle_hash: "h1", claim: "k > 0", smt2: "(assert (> k 0))" }],
      violations: [],
    });
    git(projectRoot, "add", "-A");
    git(projectRoot, "commit", "-q", "-m", "seed");

    const head = git(projectRoot, "rev-parse", "HEAD");
    const diff = new ProofDiff(projectRoot).diffAgainst(head);
    expect(diff).toEqual([]);
  });

  it("detects an added contract proof", () => {
    git(projectRoot, "init", "-q");
    git(projectRoot, "config", "user.email", "t@e.com");
    git(projectRoot, "config", "user.name", "T");

    git(projectRoot, "commit", "-q", "--allow-empty", "-m", "empty");
    const head = git(projectRoot, "rev-parse", "HEAD");

    writeContract(projectRoot, {
      key: "src/a.ts/foo[10]",
      file: "src/a.ts",
      function: "foo",
      line: 10,
      proven: [{ principle: "non-zero", principle_hash: "h1", claim: "k > 0", smt2: "(assert (> k 0))" }],
      violations: [],
    });

    const diff = new ProofDiff(projectRoot).diffAgainst(head);
    expect(diff).toHaveLength(1);
    expect(diff[0].type).toBe("added");
    expect(diff[0].claim).toBe("k > 0");
  });

  it("detects a regression: proven becomes a violation", () => {
    git(projectRoot, "init", "-q");
    git(projectRoot, "config", "user.email", "t@e.com");
    git(projectRoot, "config", "user.name", "T");

    writeContract(projectRoot, {
      key: "src/a.ts/foo[10]",
      file: "src/a.ts",
      function: "foo",
      line: 10,
      proven: [{ principle: "non-zero", principle_hash: "h1", claim: "k > 0", smt2: "(assert (> k 0))" }],
      violations: [],
    });
    git(projectRoot, "add", "-A");
    git(projectRoot, "commit", "-q", "-m", "seed-proven");
    const head = git(projectRoot, "rev-parse", "HEAD");

    // Replace with a contract where the same claim is now a violation.
    rmSync(join(projectRoot, ".provekit", "contracts"), { recursive: true });
    writeContract(projectRoot, {
      key: "src/a.ts/foo[10]",
      file: "src/a.ts",
      function: "foo",
      line: 10,
      proven: [],
      violations: [{ principle: "non-zero", principle_hash: "h1", claim: "k > 0", smt2: "(assert (> k 0))" }],
    });

    const diff = new ProofDiff(projectRoot).diffAgainst(head);
    const regressed = diff.find((d) => d.type === "regressed");
    expect(regressed).toBeDefined();
    expect(regressed!.claim).toBe("k > 0");
  });
});
