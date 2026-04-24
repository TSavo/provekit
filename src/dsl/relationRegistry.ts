/**
 * A7c: Relation registry.
 *
 * Built-in relations (before, dominates) register themselves here so the
 * compiler can resolve them without hardcoded dispatch. Adding a new
 * relation is a registerRelation() call — no compiler change required.
 *
 * Mirrors the shape of src/sast/capabilityRegistry.ts.
 */

export type RelationParamType = "node" | "literal" | "sort";

/**
 * A resolved argument passed to a relation's compile function.
 * "node" args carry the SQL alias of the nodes table row for that variable.
 * "literal" args carry a scalar value.
 * "sort" args carry a sort-name string.
 */
export type RelationArg =
  | { kind: "node"; alias: string }
  | { kind: "literal"; value: string | number | boolean | null }
  | { kind: "sort"; value: string };

export interface RelationArgs {
  /** Positional arguments. Index 0 is the first DSL param. */
  args: RelationArg[];
}

/**
 * Description of one built-in relation.
 *
 * compile() receives resolved node-table aliases (plain SQL alias strings)
 * and returns a SQL fragment string to be ANDed into the WHERE clause.
 */
export interface RelationDescriptor {
  /** Relation name as it appears in DSL (e.g., "before", "dominates"). */
  name: string;
  /** Number of positional parameters. */
  paramCount: number;
  /** Expected type of each parameter. */
  paramTypes: RelationParamType[];
  /**
   * Given resolved node-table aliases for each parameter, return a SQL
   * fragment string that asserts the relation. The fragment will be pushed
   * into the compiler's WHERE array.
   */
  compile: (args: RelationArgs) => string;
}

const registry = new Map<string, RelationDescriptor>();

/**
 * Register a relation. Idempotent: duplicate names overwrite (with a warning).
 */
export function registerRelation(d: RelationDescriptor): void {
  if (registry.has(d.name)) {
    console.warn(`[relationRegistry] duplicate registration for "${d.name}"; overwriting.`);
  }
  registry.set(d.name, d);
}

/** Look up a relation by DSL name. Returns undefined if not registered. */
export function getRelation(name: string): RelationDescriptor | undefined {
  return registry.get(name);
}

/** All registered relation descriptors (read-only snapshot). */
export function listRelations(): readonly RelationDescriptor[] {
  return Array.from(registry.values());
}

/** Clear the registry. ONLY for tests. */
export function _clearRelationRegistry(): void {
  registry.clear();
}
