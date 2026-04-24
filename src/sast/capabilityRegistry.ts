import type { SQLiteTable } from "drizzle-orm/sqlite-core";

export type SmtSort = "Real" | "Int" | "Bool" | "String" | "Array" | "BitVec";

/** Description of a single column in a capability table. */
export interface CapabilityColumn {
  /** Column name as it appears in DSL (e.g., "op", "lhs_node", "result_sort"). */
  dslName: string;
  /** Drizzle column reference, e.g. nodeArithmetic.op. Used by the compiler. */
  drizzleColumn: any; // typed as any to avoid Drizzle's deep generics; A7b will narrow
  /** SMT sort if the value is a primitive that maps to one. Optional. */
  sort?: SmtSort | "Text";
  /** True if the column FKs to nodes(id). */
  isNodeRef: boolean;
  /** Whether NULL is allowed. */
  nullable: boolean;
  /** If the column is a closed enum, the allowed values. Used for compile-time validation. */
  kindEnum?: string[];
}

/** Description of a single capability table. */
export interface CapabilityDescriptor {
  /** Capability name as it appears in DSL (e.g., "arithmetic", "assigns"). NOT the SQL table name. */
  dslName: string;
  /** The Drizzle table reference. Used by the compiler to FROM/JOIN. */
  table: SQLiteTable;
  /** Column descriptors keyed by DSL column name. */
  columns: Record<string, CapabilityColumn>;
}

const registry = new Map<string, CapabilityDescriptor>();

/**
 * Register a capability. Idempotent: duplicate names overwrite (with a warning).
 */
export function registerCapability(d: CapabilityDescriptor): void {
  if (registry.has(d.dslName)) {
    console.warn(`[capabilityRegistry] duplicate registration for "${d.dslName}"; overwriting.`);
  }
  registry.set(d.dslName, d);
}

/** Look up a capability by DSL name. Returns undefined if not registered. */
export function getCapability(name: string): CapabilityDescriptor | undefined {
  return registry.get(name);
}

/** Look up a column by capability name + DSL column name. Returns undefined if either is missing. */
export function getCapabilityColumn(capName: string, colName: string): CapabilityColumn | undefined {
  return registry.get(capName)?.columns[colName];
}

/** All registered capability descriptors (read-only snapshot). */
export function listCapabilities(): readonly CapabilityDescriptor[] {
  return Array.from(registry.values());
}

/** Clear the registry. ONLY for tests. */
export function _clearRegistry(): void {
  registry.clear();
}
