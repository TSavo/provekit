import { execFileSync } from "child_process";
import { readFileSync, existsSync } from "fs";
import { join, relative } from "path";
import { ContractStore, Contract } from "../contracts";

export interface ProofChange {
  file: string;
  function: string;
  line: number;
  type: "added" | "removed" | "regressed" | "fixed" | "unchanged";
  claim: string;
  principle: string | null;
}

export class ProofDiff {
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
  }

  diffAgainst(ref: string): ProofChange[] {
    const currentContracts = new ContractStore(this.projectRoot).getAll();
    const previousContracts = this.loadContractsAtRef(ref);

    return this.computeDiff(previousContracts, currentContracts);
  }

  private computeDiff(before: Contract[], after: Contract[]): ProofChange[] {
    const changes: ProofChange[] = [];

    const beforeMap = this.indexContracts(before);
    const afterMap = this.indexContracts(after);

    const allKeys = new Set([...beforeMap.keys(), ...afterMap.keys()]);

    for (const key of allKeys) {
      const prev = beforeMap.get(key);
      const curr = afterMap.get(key);

      if (!prev && curr) {
        for (const p of curr.proven) {
          changes.push({ file: curr.file, function: curr.function, line: curr.line, type: "added", claim: p.claim, principle: p.principle });
        }
        for (const v of curr.violations) {
          changes.push({ file: curr.file, function: curr.function, line: curr.line, type: "added", claim: `VIOLATION: ${v.claim}`, principle: v.principle });
        }
        continue;
      }

      if (prev && !curr) {
        for (const p of prev.proven) {
          changes.push({ file: prev.file, function: prev.function, line: prev.line, type: "removed", claim: p.claim, principle: p.principle });
        }
        continue;
      }

      if (prev && curr) {
        const prevProvenSet = new Set(prev.proven.map((p) => p.claim));
        const currProvenSet = new Set(curr.proven.map((p) => p.claim));
        const prevViolationSet = new Set(prev.violations.map((v) => v.claim));
        const currViolationSet = new Set(curr.violations.map((v) => v.claim));

        for (const p of curr.proven) {
          if (!prevProvenSet.has(p.claim)) {
            if (prevViolationSet.has(p.claim)) {
              changes.push({ file: curr.file, function: curr.function, line: curr.line, type: "fixed", claim: p.claim, principle: p.principle });
            } else {
              changes.push({ file: curr.file, function: curr.function, line: curr.line, type: "added", claim: p.claim, principle: p.principle });
            }
          }
        }

        for (const v of curr.violations) {
          if (!prevViolationSet.has(v.claim)) {
            if (prevProvenSet.has(v.claim)) {
              changes.push({ file: curr.file, function: curr.function, line: curr.line, type: "regressed", claim: v.claim, principle: v.principle });
            } else {
              changes.push({ file: curr.file, function: curr.function, line: curr.line, type: "added", claim: `VIOLATION: ${v.claim}`, principle: v.principle });
            }
          }
        }

        for (const p of prev.proven) {
          if (!currProvenSet.has(p.claim) && !currViolationSet.has(p.claim)) {
            changes.push({ file: prev.file, function: prev.function, line: prev.line, type: "removed", claim: p.claim, principle: p.principle });
          }
        }
      }
    }

    return changes;
  }

  private indexContracts(contracts: Contract[]): Map<string, Contract> {
    const map = new Map<string, Contract>();
    for (const c of contracts) {
      map.set(`${c.file}:${c.function}:${c.line}`, c);
    }
    return map;
  }

  private loadContractsAtRef(ref: string): Contract[] {
    const contracts: Contract[] = [];

    try {
      const files = execFileSync("git", ["ls-tree", "-r", "--name-only", ref, "--", ".neurallog/contracts/"], {
        cwd: this.projectRoot,
        encoding: "utf-8",
        stdio: ["pipe", "pipe", "pipe"],
      }).trim().split("\n").filter(Boolean);

      for (const file of files) {
        if (!file.endsWith(".json")) continue;
        try {
          const content = execFileSync("git", ["show", `${ref}:${file}`], {
            cwd: this.projectRoot,
            encoding: "utf-8",
            stdio: ["pipe", "pipe", "pipe"],
          });
          const data = JSON.parse(content);
          if (data.contracts) contracts.push(...data.contracts);
        } catch { /* skip */ }
      }
    } catch { /* no contracts at ref */ }

    return contracts;
  }
}
