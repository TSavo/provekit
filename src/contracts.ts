import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from "fs";
import { join, dirname, relative, basename } from "path";
import { createHash } from "crypto";

export interface ClauseHistory {
  clause: string;          // the SMT-LIB assertion
  status: "active" | "weakened";
  weaken_step: number;
  witness_count_at_last_weaken: number;
  current_witness_count: number;
}

export interface Contract {
  file: string;
  function: string;
  line: number;
  signal_hash: string;   // hash of the signal that generated this contract
  proven: ProvenProperty[];
  violations: Violation[];
  clause_history: ClauseHistory[];
  depends_on: string[];  // hashes of contracts that were in context during derivation
}

export interface ProvenProperty {
  principle: string | null;
  principle_hash: string;
  claim: string;
  smt2: string;
}

export interface Violation {
  principle: string | null;
  principle_hash: string;
  claim: string;
  smt2: string;
}

export interface ContractFile {
  file_hash: string;
  principle_hash?: string;
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
        for (const contract of data.contracts) {
          // Backward compat: older contract files may lack clause_history
          if (!contract.clause_history) {
            contract.clause_history = [
              ...contract.proven.map((p) => ({
                clause: p.smt2,
                status: "active" as const,
                weaken_step: 0,
                witness_count_at_last_weaken: 0,
                current_witness_count: 0,
              })),
              ...contract.violations.map((v) => ({
                clause: v.smt2,
                status: "active" as const,
                weaken_step: 0,
                witness_count_at_last_weaken: 0,
                current_witness_count: 0,
              })),
            ];
          }
        }
        this.contracts.push(...data.contracts);
      } catch {
        // skip corrupt files
      }
    }
  }

  add(contract: Contract): void {
    this.contracts.push(contract);
  }

  /**
   * Increment current_witness_count for a matching clause in the contract
   * at the given file:line. Called from the runtime transport when a proof
   * entry is recorded.
   */
  recordWitness(file: string, line: number, clause: string): void {
    const normalized = this.normalizeClause(clause);
    for (const c of this.contracts) {
      if (c.file === file && c.line === line) {
        for (const h of c.clause_history) {
          if (this.normalizeClause(h.clause) === normalized) {
            h.current_witness_count++;
            return;
          }
        }
      }
    }
  }

  private normalizeClause(s: string): string {
    return s.replace(/;[^\n]*/g, "").replace(/\s+/g, " ").trim();
  }

  /**
   * Mark a clause as weakened. Sets status to "weakened", increments
   * weaken_step, and snapshots the current witness count.
   */
  weakenClause(file: string, line: number, clause: string): void {
    for (const c of this.contracts) {
      if (c.file === file && c.line === line) {
        for (const h of c.clause_history) {
          if (h.clause === clause) {
            h.status = "weakened";
            h.weaken_step++;
            h.witness_count_at_last_weaken = h.current_witness_count;
            return;
          }
        }
      }
    }
  }

  /**
   * Returns true only if new evidence has arrived since the last weakening,
   * i.e. current_witness_count > witness_count_at_last_weaken.
   */
  canStrengthen(file: string, line: number, clause: string): boolean {
    for (const c of this.contracts) {
      if (c.file === file && c.line === line) {
        for (const h of c.clause_history) {
          if (h.clause === clause) {
            return h.current_witness_count > h.witness_count_at_last_weaken;
          }
        }
      }
    }
    return false;
  }

  writeToDisk(filePath: string, fileSource: string, principleHash?: string): void {
    const relPath = relative(this.projectRoot, filePath);
    const contractPath = join(
      this.contractsDir,
      relPath + ".json"
    );
    const dir = dirname(contractPath);
    mkdirSync(dir, { recursive: true });

    const fileHash = createHash("sha256").update(fileSource).digest("hex");
    const contractsForFile = this.contracts.filter(
      (c) => c.file === filePath || c.file === relPath
    );

    const data: ContractFile = {
      file_hash: fileHash,
      ...(principleHash ? { principle_hash: principleHash } : {}),
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

  /**
   * Compute a content hash for a contract — used as a dependency identifier.
   */
  static contractHash(contract: Contract): string {
    const content = contract.proven.map((p) => p.smt2).join("\n") +
      contract.violations.map((v) => v.smt2).join("\n");
    return createHash("sha256").update(content).digest("hex");
  }

  /**
   * Set the depends_on field for a contract based on which contracts
   * were in context during its derivation.
   */
  static withDependencies(contract: Contract, contextContracts: Contract[]): Contract {
    contract.depends_on = contextContracts.map((c) => ContractStore.contractHash(c));
    return contract;
  }
}

/**
 * Walk the dependency chain and find all contracts that are stale
 * because a dependency changed.
 *
 * A contract is stale if:
 * 1. Its own file changed (file_hash mismatch — handled by existing cache logic)
 * 2. Any contract in its depends_on list has a different hash than when
 *    the dependency was recorded (the upstream contract was re-derived)
 */
export function findStaleContracts(contracts: Contract[]): Contract[] {
  const currentHashes = new Map<string, string>();
  for (const c of contracts) {
    const key = `${c.file}:${c.function}:${c.line}`;
    currentHashes.set(key, ContractStore.contractHash(c));
  }

  const stale: Contract[] = [];
  for (const c of contracts) {
    if (!c.depends_on || c.depends_on.length === 0) continue;

    // Check if any dependency hash no longer matches a current contract
    const allCurrentHashes = new Set(currentHashes.values());
    for (const depHash of c.depends_on) {
      if (!allCurrentHashes.has(depHash)) {
        stale.push(c);
        break;
      }
    }
  }

  return stale;
}
