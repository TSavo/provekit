import { VerificationResult } from "./verifier";
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join, dirname, relative, basename } from "path";
import { createHash } from "crypto";

export interface Contract {
  file: string;
  function: string;
  line: number;
  proven: ProvenProperty[];
  violations: Violation[];
}

export interface ProvenProperty {
  principle: string | null;
  claim: string;
  smt2: string;
}

export interface Violation {
  principle: string | null;
  claim: string;
  smt2: string;
}

export interface ContractFile {
  file_hash: string;
  contracts: Contract[];
}

export class ContractStore {
  private contracts: Contract[] = [];
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.loadFromDisk();
  }

  private get neurallogDir(): string {
    return join(this.projectRoot, ".neurallog");
  }

  private get contractsDir(): string {
    return join(this.neurallogDir, "contracts");
  }

  private loadFromDisk(): void {
    if (!existsSync(this.contractsDir)) return;

    const walk = (dir: string): string[] => {
      const entries: string[] = [];
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const full = join(dir, entry.name);
        if (entry.isDirectory()) {
          entries.push(...walk(full));
        } else if (entry.name.endsWith(".json")) {
          entries.push(full);
        }
      }
      return entries;
    };

    for (const jsonPath of walk(this.contractsDir)) {
      try {
        const data: ContractFile = JSON.parse(
          readFileSync(jsonPath, "utf-8")
        );
        this.contracts.push(...data.contracts);
      } catch {
        // skip corrupt files
      }
    }
  }

  add(contract: Contract): void {
    this.contracts.push(contract);
  }

  writeToDisk(filePath: string, fileSource: string): void {
    const relPath = relative(this.projectRoot, filePath);
    const contractPath = join(
      this.contractsDir,
      relPath + ".json"
    );
    const dir = dirname(contractPath);
    mkdirSync(dir, { recursive: true });

    const fileHash = createHash("md5").update(fileSource).digest("hex");
    const contractsForFile = this.contracts.filter(
      (c) => c.file === filePath || c.file === relPath
    );

    const data: ContractFile = {
      file_hash: fileHash,
      contracts: contractsForFile,
    };

    writeFileSync(contractPath, JSON.stringify(data, null, 2));
  }

  getAll(): Contract[] {
    return [...this.contracts];
  }

  formatForPrompt(): string {
    if (this.contracts.length === 0) {
      return "(no existing contracts yet — first pass)";
    }

    const sections: string[] = [];

    for (const contract of this.contracts) {
      const lines: string[] = [];
      lines.push(
        `### ${contract.file}:${contract.function} (line ${contract.line})`
      );

      if (contract.proven.length > 0) {
        lines.push("\nProven properties (Z3 confirmed unsat):");
        for (const p of contract.proven) {
          const tag = p.principle ? `[${p.principle}]` : "";
          lines.push(`  ${tag} ${p.claim}`);
          lines.push("  ```smt2");
          lines.push(`  ${p.smt2}`);
          lines.push("  ```");
        }
      }

      if (contract.violations.length > 0) {
        lines.push("\nReachable violations (Z3 confirmed sat):");
        for (const v of contract.violations) {
          const tag = v.principle ? `[${v.principle}]` : "";
          lines.push(`  ${tag} ${v.claim}`);
        }
      }

      sections.push(lines.join("\n"));
    }

    return sections.join("\n\n");
  }

  static fromVerificationResults(
    file: string,
    functionName: string,
    line: number,
    verifications: VerificationResult[]
  ): Contract {
    const proven: ProvenProperty[] = [];
    const violations: Violation[] = [];

    for (const v of verifications) {
      const commentLines = v.smt2
        .split("\n")
        .filter((l) => l.trim().startsWith(";"))
        .map((l) => l.trim().replace(/^;\s*/, ""));

      const claim =
        commentLines.find(
          (l) =>
            !l.startsWith("PRINCIPLE:") &&
            l.length > 10
        ) || "(no claim extracted)";

      if (v.z3Result === "unsat") {
        proven.push({ principle: v.principle, claim, smt2: v.smt2 });
      } else if (v.z3Result === "sat") {
        violations.push({ principle: v.principle, claim, smt2: v.smt2 });
      }
    }

    return { file, function: functionName, line, proven, violations };
  }
}
