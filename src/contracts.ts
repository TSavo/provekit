import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, unlinkSync } from "fs";
import { join, dirname, relative, isAbsolute } from "path";
import { createHash } from "crypto";

export function normalizeContractFile(file: string, projectRoot: string): string {
  if (!isAbsolute(file)) return file;
  const rel = relative(projectRoot, file);
  if (!rel.startsWith("..") && !isAbsolute(rel)) return rel;
  for (const marker of ["/src/", "/examples/", "/lib/", "/app/", "/packages/"]) {
    const idx = file.indexOf(marker);
    if (idx >= 0) return file.slice(idx + 1);
  }
  return file;
}

export interface ClauseHistory {
  clause: string;
  status: "active" | "weakened";
  weaken_step: number;
  witness_count_at_last_weaken: number;
  current_witness_count: number;
}

export interface ProvenProperty {
  principle: string | null;
  principle_hash: string;
  claim: string;
  smt2: string;
  reason?: string;
  confidence?: "high" | "low";
  judge_note?: string;
}

export interface SmtBinding {
  smt_constant: string;
  source_line: number;
  source_expr: string;
  sort: string;
}

export interface Violation {
  principle: string | null;
  principle_hash: string;
  claim: string;
  smt2: string;
  witness?: string;
  // Per-constant binding metadata emitted by the LLM alongside the smt2 block.
  // Absent on legacy contracts derived before the binding prompt shipped; the
  // gap detector skips violations without bindings.
  smt_bindings?: SmtBinding[];
  complexity?: number;
  confidence?: "high" | "low";
  reason?: string;
  judge_note?: string;
}

export interface Contract {
  key: string;
  file: string;
  function: string;
  line: number;
  signal_hash: string;
  proven: ProvenProperty[];
  violations: Violation[];
  clause_history: ClauseHistory[];
  depends_on: string[];
}

export function signalKey(file: string, fn: string, line: number): string {
  const normalized = file.replace(/\\/g, "/").replace(/^\/+/, "");
  return `${normalized}/${fn}[${line}]`;
}

export function signalKeyToPath(key: string): string {
  return key + ".json";
}

export function contractHash(contract: Contract): string {
  const content = contract.proven.map((p) => p.smt2).join("\n") +
    contract.violations.map((v) => v.smt2).join("\n");
  return createHash("sha256").update(content).digest("hex");
}

export class ContractStore {
  private contracts: Map<string, Contract> = new Map();
  private projectRoot: string;

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.loadFromDisk();
  }

  private get contractsDir(): string {
    return join(this.projectRoot, ".neurallog", "contracts");
  }

  private loadFromDisk(): void {
    if (!existsSync(this.contractsDir)) return;

    const walk = (dir: string): string[] => {
      const entries: string[] = [];
      try {
        for (const entry of readdirSync(dir, { withFileTypes: true })) {
          const full = join(dir, entry.name);
          if (entry.isDirectory()) {
            entries.push(...walk(full));
          } else if (entry.name.endsWith(".json")) {
            entries.push(full);
          }
        }
      } catch {}
      return entries;
    };

    for (const jsonPath of walk(this.contractsDir)) {
      try {
        const raw = readFileSync(jsonPath, "utf-8");
        const data = JSON.parse(raw);

        if (data.key) {
          const contract: Contract = data;
          contract.file = normalizeContractFile(contract.file, this.projectRoot);
          if (!contract.clause_history) {
            contract.clause_history = [
              ...contract.proven.map((p) => ({ clause: p.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
              ...contract.violations.map((v) => ({ clause: v.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
            ];
          }
          this.contracts.set(contract.key, contract);
        } else if (data.contracts) {
          for (const contract of data.contracts) {
            contract.file = normalizeContractFile(contract.file, this.projectRoot);
            const key = contract.key || signalKey(contract.file, contract.function, contract.line);
            contract.key = key;
            if (!contract.clause_history) {
              contract.clause_history = [
                ...contract.proven.map((p: any) => ({ clause: p.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
                ...contract.violations.map((v: any) => ({ clause: v.smt2, status: "active" as const, weaken_step: 0, witness_count_at_last_weaken: 0, current_witness_count: 0 })),
              ];
            }
            this.contracts.set(key, contract);
          }
        }
      } catch {}
    }

    console.log(`[contracts] Loaded ${this.contracts.size} contracts from disk`);
  }

  get(key: string): Contract | undefined {
    return this.contracts.get(key);
  }

  has(key: string): boolean {
    return this.contracts.has(key);
  }

  put(contract: Contract): void {
    this.contracts.set(contract.key, contract);
    this.writeSingle(contract);
  }

  remove(key: string): void {
    this.contracts.delete(key);
    const filePath = join(this.contractsDir, signalKeyToPath(key));
    try { unlinkSync(filePath); } catch (e: any) { console.log(`[contracts] remove ${key}: ${e?.message?.slice(0, 40) || "ok"}`); }
  }

  getAll(): Contract[] {
    return [...this.contracts.values()];
  }

  getByFile(file: string): Contract[] {
    return this.getAll().filter((c) => c.file === file);
  }

  getDependents(key: string): Contract[] {
    return this.getAll().filter((c) => c.depends_on.includes(key));
  }

  findStale(): Contract[] {
    const stale: Contract[] = [];
    const staleKeys = new Set<string>();

    for (const c of this.contracts.values()) {
      for (const dep of c.depends_on) {
        if (!this.contracts.has(dep)) {
          stale.push(c);
          staleKeys.add(c.key);
          break;
        }
      }
    }

    let cascaded = true;
    while (cascaded) {
      cascaded = false;
      for (const c of this.contracts.values()) {
        if (staleKeys.has(c.key)) continue;
        for (const dep of c.depends_on) {
          if (staleKeys.has(dep)) {
            stale.push(c);
            staleKeys.add(c.key);
            cascaded = true;
            break;
          }
        }
      }
    }

    return stale;
  }

  formatForPrompt(keys?: string[]): string {
    const contracts = keys
      ? keys.map((k) => this.contracts.get(k)).filter((c): c is Contract => !!c)
      : this.getAll();

    if (contracts.length === 0) return "(no existing contracts)";

    return contracts.map((c) => {
      const lines: string[] = [];
      lines.push(`### ${c.key}`);
      if (c.proven.length > 0) {
        lines.push("\nProven (Z3 unsat):");
        for (const p of c.proven) {
          const tag = p.principle ? `[${p.principle}]` : "";
          lines.push(`  ${tag} ${p.claim}`);
        }
      }
      if (c.violations.length > 0) {
        lines.push("\nViolations (Z3 sat):");
        for (const v of c.violations) {
          const tag = v.principle ? `[${v.principle}]` : "";
          lines.push(`  ${tag} ${v.claim}`);
        }
      }
      return lines.join("\n");
    }).join("\n\n");
  }

  private writeSingle(contract: Contract): void {
    const filePath = join(this.contractsDir, signalKeyToPath(contract.key));
    const dir = dirname(filePath);
    mkdirSync(dir, { recursive: true });
    writeFileSync(filePath, JSON.stringify(contract, null, 2));
  }

  recordWitness(key: string, clause: string): void {
    const c = this.contracts.get(key);
    if (!c) return;
    const normalized = clause.replace(/;[^\n]*/g, "").replace(/\s+/g, " ").trim();
    for (const h of c.clause_history) {
      if (h.clause.replace(/;[^\n]*/g, "").replace(/\s+/g, " ").trim() === normalized) {
        h.current_witness_count++;
        return;
      }
    }
  }
}
