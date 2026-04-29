/**
 * Workflow manifests — workflows as data, loaded from YAML.
 *
 * Spec: docs/specs/2026-04-29-workflows-as-first-class-primitive.md
 *
 * The manifest is a graph of capability calls. Each node names the
 * capability to invoke and the inputs to feed it. Inputs are a small
 * reference language (`$input`, `$node.<id>.output`, composed objects,
 * literals) that lets later nodes consume earlier nodes' outputs.
 *
 * The runner topo-sorts the graph, executes nodes in dependency order,
 * threads outputs forward, collects upstream CIDs into inputCids for
 * the memento DAG, and ultimately returns the terminal node's output
 * inside a workflow-level memento.
 *
 * Why YAML: workflows are human-authored documents that need comments,
 * cross-language portability, and editability outside a TS toolchain.
 * The TS interface here is the canonical typed shape; YAML is the
 * canonical wire/storage format.
 */

import { parse as parseYaml } from "yaml";
import type { ProducerRegistry } from "./registry.js";
import type { StageResult, Workflow } from "./types.js";
import type { WorkflowRunner } from "./runner.js";

// ---------------------------------------------------------------------------
// Manifest types — canonical TS shape; mirrors the YAML schema.
// ---------------------------------------------------------------------------

export interface WorkflowManifest {
  /** Workflow name (free-form, used in producedBy and telemetry). */
  name: string;
  /** Content hash of the workflow definition. Bumps when the manifest changes. */
  cid: string;
  /** Optional human-readable description. Carried through but not used by the runner. */
  description?: string;
  /** Nodes in the DAG. Order is irrelevant; runner topo-sorts. */
  nodes: NodeSpec[];
  /** Reference to the terminal node whose output is the workflow output. */
  output: InputRef;
}

export interface NodeSpec {
  /** Unique within the manifest. Other nodes reference this id. */
  id: string;
  /** Capability the registry must resolve. */
  capability: string;
  /** What feeds the node. May reference $input or prior nodes. */
  input: InputSpec;
}

/**
 * Input shape. Strings starting with $ are references; everything else
 * is a literal. Objects are recursed; arrays preserved.
 */
export type InputSpec = InputRef | InputObject | InputArray | InputLiteral;

/** A reference. Examples: "$input", "$input.text", "$node.intake.output". */
export type InputRef = string;

/** Object whose values are themselves InputSpecs. */
export interface InputObject {
  [key: string]: InputSpec;
}

/** Array of InputSpecs. */
export type InputArray = InputSpec[];

/** Anything else — number, boolean, null, plain string without leading $. */
export type InputLiteral = number | boolean | null;

// ---------------------------------------------------------------------------
// Parse + validate
// ---------------------------------------------------------------------------

/**
 * Parse a YAML string into a WorkflowManifest. Throws with a clear
 * message if the document doesn't have the required shape.
 */
export function parseManifest(yamlText: string): WorkflowManifest {
  const doc = parseYaml(yamlText);
  return validateManifest(doc);
}

/**
 * Structural validation. Confirms the manifest has the required fields,
 * node ids are unique, references resolve to declared nodes or $input,
 * and the dependency graph is acyclic.
 */
export function validateManifest(value: unknown): WorkflowManifest {
  if (!value || typeof value !== "object") {
    throw new Error("manifest must be an object");
  }
  const m = value as Record<string, unknown>;
  if (typeof m.name !== "string") throw new Error("manifest.name must be string");
  if (typeof m.cid !== "string") throw new Error("manifest.cid must be string");
  if (!Array.isArray(m.nodes)) throw new Error("manifest.nodes must be array");
  if (typeof m.output !== "string" || !isReference(m.output)) {
    throw new Error("manifest.output must be a $-prefixed reference");
  }

  const seenIds = new Set<string>();
  const nodes: NodeSpec[] = [];
  for (const raw of m.nodes) {
    if (!raw || typeof raw !== "object") {
      throw new Error("each node must be an object");
    }
    const n = raw as Record<string, unknown>;
    if (typeof n.id !== "string") throw new Error("node.id must be string");
    if (seenIds.has(n.id)) throw new Error(`duplicate node id "${n.id}"`);
    seenIds.add(n.id);
    if (typeof n.capability !== "string") {
      throw new Error(`node "${n.id}" capability must be string`);
    }
    if (n.input === undefined) {
      throw new Error(`node "${n.id}" missing input`);
    }
    nodes.push({
      id: n.id,
      capability: n.capability,
      input: n.input as InputSpec,
    });
  }

  // Validate references in inputs and the output reference.
  for (const node of nodes) {
    for (const ref of collectReferences(node.input)) {
      assertReferenceValid(ref, seenIds, `node "${node.id}".input`);
    }
  }
  assertReferenceValid(m.output, seenIds, "manifest.output");

  // Acyclicity: topo sort throws if a cycle exists.
  topoSort(nodes);

  return {
    name: m.name,
    cid: m.cid,
    description: typeof m.description === "string" ? m.description : undefined,
    nodes,
    output: m.output,
  };
}

function assertReferenceValid(
  ref: string,
  declaredIds: Set<string>,
  context: string,
): void {
  const parsed = parseReference(ref);
  if (parsed.kind === "input") return;
  if (!declaredIds.has(parsed.nodeId)) {
    throw new Error(
      `${context}: reference "${ref}" points at undeclared node "${parsed.nodeId}"`,
    );
  }
  if (parsed.field !== "output") {
    throw new Error(
      `${context}: reference "${ref}" must end in .output (only stage outputs are referenceable)`,
    );
  }
}

// ---------------------------------------------------------------------------
// Reference language
// ---------------------------------------------------------------------------

type ParsedRef =
  | { kind: "input"; path: string[] }
  | { kind: "node"; nodeId: string; field: string; path: string[] };

function isReference(value: unknown): value is string {
  return typeof value === "string" && value.startsWith("$");
}

function parseReference(ref: string): ParsedRef {
  if (!ref.startsWith("$")) {
    throw new Error(`not a reference: "${ref}"`);
  }
  const body = ref.slice(1);
  const parts = body.split(".");
  if (parts[0] === "input") {
    return { kind: "input", path: parts.slice(1) };
  }
  if (parts[0] === "node") {
    if (parts.length < 3) {
      throw new Error(`malformed node reference: "${ref}" (expected $node.<id>.<field>)`);
    }
    return {
      kind: "node",
      nodeId: parts[1],
      field: parts[2],
      path: parts.slice(3),
    };
  }
  throw new Error(`unrecognized reference root: "${ref}" (expected $input or $node)`);
}

function collectReferences(input: InputSpec): string[] {
  const out: string[] = [];
  walkInput(input, (v) => {
    if (isReference(v)) out.push(v);
  });
  return out;
}

function walkInput(
  input: InputSpec,
  visit: (value: unknown) => void,
): void {
  visit(input);
  if (Array.isArray(input)) {
    for (const item of input) walkInput(item, visit);
  } else if (input && typeof input === "object") {
    for (const v of Object.values(input as InputObject)) walkInput(v, visit);
  }
}

// ---------------------------------------------------------------------------
// Topo sort
// ---------------------------------------------------------------------------

/**
 * Kahn's algorithm. Returns nodes in execution order. Throws on cycle.
 */
export function topoSort(nodes: NodeSpec[]): NodeSpec[] {
  const byId = new Map<string, NodeSpec>(nodes.map((n) => [n.id, n]));
  const dependsOn = new Map<string, Set<string>>();
  const dependedBy = new Map<string, Set<string>>();

  for (const node of nodes) {
    dependsOn.set(node.id, new Set());
    dependedBy.set(node.id, new Set());
  }
  for (const node of nodes) {
    for (const ref of collectReferences(node.input)) {
      const parsed = parseReference(ref);
      if (parsed.kind === "node") {
        dependsOn.get(node.id)!.add(parsed.nodeId);
        dependedBy.get(parsed.nodeId)!.add(node.id);
      }
    }
  }

  const ready: string[] = [];
  for (const [id, deps] of dependsOn) {
    if (deps.size === 0) ready.push(id);
  }

  const order: NodeSpec[] = [];
  while (ready.length > 0) {
    const id = ready.shift()!;
    order.push(byId.get(id)!);
    for (const downstream of dependedBy.get(id)!) {
      const deps = dependsOn.get(downstream)!;
      deps.delete(id);
      if (deps.size === 0) ready.push(downstream);
    }
  }

  if (order.length < nodes.length) {
    const stuck = nodes.filter((n) => !order.includes(n)).map((n) => n.id);
    throw new Error(`cycle detected in workflow nodes: ${stuck.join(", ")}`);
  }
  return order;
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

interface NodeRecord {
  output: unknown;
  cid: string;
}

/**
 * Execute a workflow manifest. Topo-sorts nodes, calls the runner's
 * registry-aware request() for each, threads outputs and CIDs through
 * the dependency graph, wraps the whole thing in a workflow-level
 * memento via runner.runWorkflow().
 */
export async function runManifest(
  runner: WorkflowRunner,
  registry: ProducerRegistry,
  manifest: WorkflowManifest,
  workflowInput: unknown,
): Promise<StageResult<unknown>> {
  // Surface unknown capabilities up front rather than mid-run.
  const known = new Set(registry.capabilities());
  for (const node of manifest.nodes) {
    if (!known.has(node.capability)) {
      throw new Error(
        `manifest "${manifest.name}" references capability "${node.capability}" which is not registered`,
      );
    }
  }

  const order = topoSort(manifest.nodes);

  return runner.runWorkflow(workflowInput, async (r) => {
    const records = new Map<string, NodeRecord>();
    for (const node of order) {
      const resolvedInput = resolveInput(node.input, workflowInput, records);
      const inputCids = collectInputCids(node.input, records);
      const result = await r.request(node.capability, resolvedInput, inputCids);
      records.set(node.id, { output: result.output, cid: result.cid });
    }
    const terminal = parseReference(manifest.output);
    if (terminal.kind !== "node") {
      throw new Error(`manifest.output must reference a node, got "${manifest.output}"`);
    }
    const record = records.get(terminal.nodeId);
    if (!record) {
      throw new Error(`terminal node "${terminal.nodeId}" produced no record`);
    }
    return { output: record.output, cid: record.cid };
  });
}

function resolveInput(
  input: InputSpec,
  workflowInput: unknown,
  records: Map<string, NodeRecord>,
): unknown {
  if (isReference(input)) {
    return resolveReference(input, workflowInput, records);
  }
  if (Array.isArray(input)) {
    return input.map((item) => resolveInput(item, workflowInput, records));
  }
  if (input && typeof input === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(input as InputObject)) {
      out[k] = resolveInput(v, workflowInput, records);
    }
    return out;
  }
  return input;
}

function resolveReference(
  ref: string,
  workflowInput: unknown,
  records: Map<string, NodeRecord>,
): unknown {
  const parsed = parseReference(ref);
  if (parsed.kind === "input") {
    return walkPath(workflowInput, parsed.path);
  }
  const record = records.get(parsed.nodeId);
  if (!record) {
    throw new Error(`reference "${ref}" used before node "${parsed.nodeId}" produced output`);
  }
  if (parsed.field !== "output") {
    throw new Error(`reference "${ref}" — only "output" is currently a valid field`);
  }
  return walkPath(record.output, parsed.path);
}

function walkPath(value: unknown, path: string[]): unknown {
  let current: unknown = value;
  for (const segment of path) {
    if (current === null || current === undefined) return undefined;
    if (typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function collectInputCids(
  input: InputSpec,
  records: Map<string, NodeRecord>,
): string[] {
  const cids: string[] = [];
  for (const ref of collectReferences(input)) {
    const parsed = parseReference(ref);
    if (parsed.kind === "node") {
      const record = records.get(parsed.nodeId);
      if (record) cids.push(record.cid);
    }
  }
  // De-duplicate while preserving first-seen order.
  return [...new Set(cids)];
}

// ---------------------------------------------------------------------------
// Convenience: turn a manifest into a Workflow handle
// ---------------------------------------------------------------------------

/**
 * Manifest → Workflow handle. Pass into `new WorkflowRunner(db, this, ...)`.
 */
export function manifestToWorkflow(manifest: WorkflowManifest): Workflow {
  return { name: manifest.name, cid: manifest.cid };
}
