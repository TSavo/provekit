/**
 * Shared types for the TS lift toolchain.
 *
 * Mirrors the Rust `provekit-lift` crate's `LiftReport`/`AdapterReport`/
 * `AdapterWarning` shapes one-for-one. The TS impl keeps adapters in a
 * single package (no per-adapter crate split) because TS doesn't need
 * the workspace-level isolation Cargo affords.
 *
 * STRATEGIC POSITIONING (read this before extending):
 *
 *   ProvekIt does NOT compete with `zod`, `fast-check`, `io-ts`, `yup`,
 *   `joi`, `class-validator`, `valibot`, etc. It sits BENEATH them.
 *   Developers keep their existing schema/property library; the lift
 *   adapters in this directory READ what's already there and promote
 *   each construct to a content-addressed signed contract memento.
 *
 *   The `proveLift/` LLM-driven pipeline is a fallback for greenfield
 *   code where no annotation library is in use. Lift first; LLM-mint
 *   only when greenfield.
 */
import type { IrFormula } from "../ir/formulas.js";

/**
 * A single lifted contract declaration. Mirrors Rust's `ContractDecl`.
 * Carries any combination of pre/post/inv with at least one slot set.
 */
export interface ContractDecl {
  /** Stable, source-derived contract name. */
  name: string;
  /** Variable name the post-formula uses to reference the return value. */
  outBinding: string;
  /** Source path the contract was lifted from (for warnings/debug). */
  sourcePath: string;
  /** Adapter that produced this decl (e.g., "zod", "fast-check"). */
  adapter: string;
  pre?: IrFormula;
  post?: IrFormula;
  inv?: IrFormula;
  /**
   * When present, this contract should be bridged to the named
   * contract in the given kit. Format: "<kit>:<contractName>"
   * (e.g., "openapi:e2e-api-1-0-0-get-getusers-200-application-json").
   * Populated by the provekit-annotations adapter.
   */
  targetContract?: string;
}

export interface AdapterWarning {
  adapter: string;
  sourcePath: string;
  itemName: string;
  reason: string;
}

export interface AdapterOutput {
  decls: ContractDecl[];
  /** Total candidate items the adapter saw (lifted + skipped). */
  seen: number;
  /** Items successfully lifted to a ContractDecl. */
  lifted: number;
  warnings: AdapterWarning[];
}

export interface AdapterReport {
  adapter: string;
  seen: number;
  lifted: number;
  warnings: AdapterWarning[];
}

export interface LiftReport {
  decls: ContractDecl[];
  adapterReports: AdapterReport[];
  filesScanned: number;
  parseErrors: Array<{ path: string; message: string }>;
}
