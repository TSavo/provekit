/**
 * Workflow manifests — workflows as data, loaded from YAML.
 *
 * Spec: docs/specs/2026-04-29-workflows-as-first-class-primitive.md
 *       docs/specs/2026-04-29-stages-vs-actions.md
 *       docs/specs/2026-04-29-per-language-kit-standard.md
 *
 * The manifest is a graph of capability calls. Each node names the
 * capability to invoke and the inputs to feed it. Inputs are a small
 * reference language (`$input`, `$node.<id>.output`,
 * `$action.<id>.resource`, composed objects, literals) that lets later
 * nodes consume earlier nodes' outputs and action resources.
 *
 * The runner topo-sorts the graph, executes nodes in dependency order,
 * threads outputs forward, collects upstream Stage CIDs into inputCids
 * for the memento DAG, and ultimately returns the terminal node's
 * output inside a workflow-level memento. Action audit CIDs are NOT
 * mixed into Stage inputCids — the proof DAG and audit DAG stay
 * separate.
 *
 * Why YAML: workflows are human-authored documents that need comments,
 * cross-language portability, and editability outside a TS toolchain.
 * The TS interface here is the canonical typed shape; YAML is the
 * canonical wire/storage format.
 */

import { readFileSync, existsSync } from "fs";
import { join } from "path";
import { parse as parseYaml } from "yaml";
import type { ActionRegistry, ProducerRegistry } from "./registry.js";
import type { ActionResult, StageResult, Workflow } from "./types.js";
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
  /** Stage nodes. Order is irrelevant; runner topo-sorts. */
  nodes: NodeSpec[];
  /**
   * Optional Action nodes (side-effecting, run-every-time). Defaults to
   * empty when absent in YAML.
   */
  actions: ActionSpec[];
  /**
   * Reference to the terminal STAGE node whose output is the workflow
   * output. Action resources cannot be a terminal output — they are
   * uncacheable live handles.
   */
  output: InputRef;
}

export interface NodeSpec {
  /** Unique within the manifest. Other nodes reference this id. */
  id: string;
  /** Capability the registry must resolve. */
  capability: string;
  /** What feeds the node. May reference $input, $node, or $action. */
  input: InputSpec;
}

export interface ActionSpec {
  /** Unique within the manifest (across both nodes and actions). */
  id: string;
  /** Action capability name. */
  action: string;
  /** What feeds the action. May reference $input, $node, or $action. */
  input: InputSpec;
  /**
   * Optional ordering constraint. A 2-part reference of the form
   * `$node.<id>` or `$action.<id>` — the action will not start until
   * the named node/action has completed. This is an ordering edge,
   * not a data dependency: the resolved value is NOT threaded into
   * the action's input.
   */
  runAfter?: string;
}

/**
 * Input shape. Strings starting with $ are references; everything else
 * is a literal. Objects are recursed; arrays preserved.
 */
export type InputSpec = InputRef | InputObject | InputArray | InputLiteral;

/** A reference. Examples: "$input", "$input.text", "$node.intake.output", "$action.open-overlay.resource". */
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
 * node ids are unique (across both stages and actions), references
 * resolve to declared ids, action references use `.resource` not
 * `.output`, the workflow output points at a Stage node, and the
 * dependency graph is acyclic.
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
  if (m.actions !== undefined && !Array.isArray(m.actions)) {
    throw new Error("manifest.actions must be array when present");
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

  const actions: ActionSpec[] = [];
  const actionIds = new Set<string>();
  if (Array.isArray(m.actions)) {
    for (const raw of m.actions) {
      if (!raw || typeof raw !== "object") {
        throw new Error("each action must be an object");
      }
      const a = raw as Record<string, unknown>;
      if (typeof a.id !== "string") throw new Error("action.id must be string");
      if (seenIds.has(a.id)) {
        throw new Error(
          `duplicate id "${a.id}" — node and action ids must be unique across the manifest`,
        );
      }
      seenIds.add(a.id);
      actionIds.add(a.id);
      if (typeof a.action !== "string") {
        throw new Error(`action "${a.id}" .action capability must be string`);
      }
      if (a.input === undefined) {
        throw new Error(`action "${a.id}" missing input`);
      }
      let runAfter: string | undefined;
      if (a.runAfter !== undefined) {
        if (typeof a.runAfter !== "string" || !isReference(a.runAfter)) {
          throw new Error(
            `action "${a.id}" runAfter must be a $-prefixed reference`,
          );
        }
        runAfter = a.runAfter;
      }
      actions.push({
        id: a.id,
        action: a.action,
        input: a.input as InputSpec,
        runAfter,
      });
    }
  }

  const nodeIds = new Set(nodes.map((n) => n.id));

  // Validate references in inputs.
  for (const node of nodes) {
    for (const ref of collectReferences(node.input)) {
      assertReferenceValid(ref, nodeIds, actionIds, `node "${node.id}".input`);
    }
  }
  for (const action of actions) {
    for (const ref of collectReferences(action.input)) {
      assertReferenceValid(
        ref,
        nodeIds,
        actionIds,
        `action "${action.id}".input`,
      );
    }
    if (action.runAfter !== undefined) {
      assertRunAfterReferenceValid(
        action.runAfter,
        nodeIds,
        actionIds,
        `action "${action.id}".runAfter`,
      );
    }
  }

  // The workflow's terminal output must be a Stage node — Action
  // resources are uncacheable live handles and cannot serve as the
  // workflow's output.
  const terminal = parseReference(m.output);
  if (terminal.kind === "input") {
    throw new Error(
      `manifest.output must reference a node, got "${m.output}"`,
    );
  }
  if (terminal.kind === "action") {
    throw new Error(
      `manifest.output must reference a stage node, got action reference "${m.output}" — action resources are not cacheable workflow outputs`,
    );
  }
  assertReferenceValid(m.output, nodeIds, actionIds, "manifest.output");

  // Acyclicity: topo sort throws if a cycle exists.
  topoSort(nodes, actions);

  return {
    name: m.name,
    cid: m.cid,
    description: typeof m.description === "string" ? m.description : undefined,
    nodes,
    actions,
    output: m.output,
  };
}

function assertReferenceValid(
  ref: string,
  nodeIds: Set<string>,
  actionIds: Set<string>,
  context: string,
): void {
  const parsed = parseReference(ref);
  if (parsed.kind === "input") return;
  if (parsed.kind === "node") {
    if (!nodeIds.has(parsed.nodeId)) {
      throw new Error(
        `${context}: reference "${ref}" points at undeclared node "${parsed.nodeId}"`,
      );
    }
    if (parsed.field !== "output") {
      throw new Error(
        `${context}: reference "${ref}" must end in .output (only stage outputs are referenceable)`,
      );
    }
    return;
  }
  // action
  if (!actionIds.has(parsed.actionId)) {
    throw new Error(
      `${context}: reference "${ref}" points at undeclared action "${parsed.actionId}"`,
    );
  }
  if (parsed.field !== "resource") {
    throw new Error(
      `${context}: action reference "${ref}" is invalid — action references must end in .resource (actions do not have .output)`,
    );
  }
}

function assertRunAfterReferenceValid(
  ref: string,
  nodeIds: Set<string>,
  actionIds: Set<string>,
  context: string,
): void {
  const parsed = parseRunAfterReference(ref);
  if (parsed.kind === "node") {
    if (!nodeIds.has(parsed.id)) {
      throw new Error(
        `${context}: reference "${ref}" points at undeclared node "${parsed.id}"`,
      );
    }
    return;
  }
  if (!actionIds.has(parsed.id)) {
    throw new Error(
      `${context}: reference "${ref}" points at undeclared action "${parsed.id}"`,
    );
  }
}

// ---------------------------------------------------------------------------
// Reference language
// ---------------------------------------------------------------------------

type ParsedRef =
  | { kind: "input"; path: string[] }
  | { kind: "node"; nodeId: string; field: string; path: string[] }
  | { kind: "action"; actionId: string; field: string; path: string[] };

type ParsedRunAfterRef =
  | { kind: "node"; id: string }
  | { kind: "action"; id: string };

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
  if (parts[0] === "action") {
    if (parts.length < 3) {
      throw new Error(
        `malformed action reference: "${ref}" (expected $action.<id>.<field>)`,
      );
    }
    return {
      kind: "action",
      actionId: parts[1],
      field: parts[2],
      path: parts.slice(3),
    };
  }
  throw new Error(
    `unrecognized reference root: "${ref}" (expected $input, $node, or $action)`,
  );
}

/**
 * runAfter references are 2-part: `$node.<id>` or `$action.<id>` with
 * no trailing field. They express ordering, not data flow.
 */
function parseRunAfterReference(ref: string): ParsedRunAfterRef {
  if (!ref.startsWith("$")) {
    throw new Error(`not a reference: "${ref}"`);
  }
  const body = ref.slice(1);
  const parts = body.split(".");
  if (parts.length !== 2) {
    throw new Error(
      `malformed runAfter reference: "${ref}" (expected $node.<id> or $action.<id>)`,
    );
  }
  if (parts[0] === "node") {
    return { kind: "node", id: parts[1] };
  }
  if (parts[0] === "action") {
    return { kind: "action", id: parts[1] };
  }
  throw new Error(
    `unrecognized runAfter root: "${ref}" (expected $node or $action)`,
  );
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

export type ScheduledEntry =
  | { kind: "node"; spec: NodeSpec }
  | { kind: "action"; spec: ActionSpec };

/**
 * Kahn's algorithm over a mixed Stage/Action graph. Returns entries
 * in execution order. Throws on cycle.
 *
 * Edge rules:
 *  - Stage referencing $node.<id>.output       → depends on that Stage.
 *  - Stage referencing $action.<id>.resource   → depends on that Action.
 *  - Action referencing $node.<id>.output      → depends on that Stage.
 *  - Action referencing $action.<id>.resource  → depends on that Action.
 *  - Action runAfter: $node.<id>               → depends on that Stage.
 *  - Action runAfter: $action.<id>             → depends on that Action.
 */
export function topoSort(
  nodes: NodeSpec[],
  actions: ActionSpec[] = [],
): ScheduledEntry[] {
  // Keys are namespaced as "node:<id>" and "action:<id>" so a node and
  // an action can share an id (we forbid that in validateManifest, but
  // the topo machinery is robust regardless).
  const nodeKey = (id: string) => `node:${id}`;
  const actionKey = (id: string) => `action:${id}`;

  const byKey = new Map<string, ScheduledEntry>();
  for (const n of nodes) byKey.set(nodeKey(n.id), { kind: "node", spec: n });
  for (const a of actions)
    byKey.set(actionKey(a.id), { kind: "action", spec: a });

  const dependsOn = new Map<string, Set<string>>();
  const dependedBy = new Map<string, Set<string>>();
  for (const key of byKey.keys()) {
    dependsOn.set(key, new Set());
    dependedBy.set(key, new Set());
  }

  const addEdge = (from: string, to: string) => {
    if (!dependsOn.has(from) || !dependsOn.has(to)) return;
    dependsOn.get(from)!.add(to);
    dependedBy.get(to)!.add(from);
  };

  for (const node of nodes) {
    for (const ref of collectReferences(node.input)) {
      const parsed = parseReference(ref);
      if (parsed.kind === "node") {
        addEdge(nodeKey(node.id), nodeKey(parsed.nodeId));
      } else if (parsed.kind === "action") {
        addEdge(nodeKey(node.id), actionKey(parsed.actionId));
      }
    }
  }
  for (const action of actions) {
    for (const ref of collectReferences(action.input)) {
      const parsed = parseReference(ref);
      if (parsed.kind === "node") {
        addEdge(actionKey(action.id), nodeKey(parsed.nodeId));
      } else if (parsed.kind === "action") {
        addEdge(actionKey(action.id), actionKey(parsed.actionId));
      }
    }
    if (action.runAfter !== undefined) {
      const parsed = parseRunAfterReference(action.runAfter);
      if (parsed.kind === "node") {
        addEdge(actionKey(action.id), nodeKey(parsed.id));
      } else {
        addEdge(actionKey(action.id), actionKey(parsed.id));
      }
    }
  }

  const ready: string[] = [];
  for (const [key, deps] of dependsOn) {
    if (deps.size === 0) ready.push(key);
  }

  const order: ScheduledEntry[] = [];
  while (ready.length > 0) {
    const key = ready.shift()!;
    order.push(byKey.get(key)!);
    for (const downstream of dependedBy.get(key)!) {
      const deps = dependsOn.get(downstream)!;
      deps.delete(key);
      if (deps.size === 0) ready.push(downstream);
    }
  }

  if (order.length < byKey.size) {
    const stuck = [...byKey.keys()].filter(
      (k) => !order.some((e) => keyOf(e) === k),
    );
    throw new Error(`cycle detected in workflow graph: ${stuck.join(", ")}`);
  }
  return order;
}

function keyOf(entry: ScheduledEntry): string {
  return entry.kind === "node"
    ? `node:${entry.spec.id}`
    : `action:${entry.spec.id}`;
}

// ---------------------------------------------------------------------------
// Runner
// ---------------------------------------------------------------------------

interface NodeRecord {
  output: unknown;
  cid: string;
}

/**
 * Execute a workflow manifest. Topo-sorts the mixed Stage/Action graph,
 * dispatches Stages via the producer registry (cache-aware) and
 * Actions via the action registry (cache-bypassing), threads outputs
 * and resources forward, and wraps the whole thing in a workflow-level
 * memento via runner.runWorkflow().
 *
 * QUIET PART: Resource fields from $action references are passed to
 * Stage.run() but must NOT appear in serializeInput() return values.
 * This is enforced by Stage author discipline, not by the parser —
 * the parser does not know which input fields a given Stage's
 * serializeInput will hash. The TypeScript type system (the fact that
 * serializeInput returns `unknown`) is the constraint.
 *
 * Action audit CIDs are NOT mixed into Stage inputCids. The proof DAG
 * (walkable from any Stage memento) and the audit DAG (which includes
 * action invocations) stay disjoint by construction.
 */
export async function runManifest(
  runner: WorkflowRunner,
  registry: ProducerRegistry,
  manifest: WorkflowManifest,
  workflowInput: unknown,
  actionRegistry?: ActionRegistry,
): Promise<StageResult<unknown>> {
  // Surface unknown stage capabilities up front rather than mid-run.
  const knownStages = new Set(registry.capabilities());
  for (const node of manifest.nodes) {
    if (!knownStages.has(node.capability)) {
      throw new Error(
        `manifest "${manifest.name}" references capability "${node.capability}" which is not registered`,
      );
    }
  }
  // Surface unknown action capabilities up front when actions exist.
  if (manifest.actions.length > 0) {
    if (!actionRegistry) {
      throw new Error(
        `manifest "${manifest.name}" declares actions but runManifest was called without an actionRegistry`,
      );
    }
    const knownActions = new Set(actionRegistry.capabilities());
    for (const action of manifest.actions) {
      if (!knownActions.has(action.action)) {
        throw new Error(
          `manifest "${manifest.name}" references action capability "${action.action}" which is not registered`,
        );
      }
    }
  }

  const order = topoSort(manifest.nodes, manifest.actions);

  return runner.runWorkflow(workflowInput, async (r) => {
    const records = new Map<string, NodeRecord>();
    const resources = new Map<string, unknown>();

    for (const entry of order) {
      if (entry.kind === "node") {
        const node = entry.spec;
        const resolvedInput = resolveInput(
          node.input,
          workflowInput,
          records,
          resources,
        );
        const inputCids = collectInputCids(node.input, records);
        const result = await r.request(
          node.capability,
          resolvedInput,
          inputCids,
        );
        records.set(node.id, { output: result.output, cid: result.cid });
      } else {
        const action = entry.spec;
        const resolvedInput = resolveInput(
          action.input,
          workflowInput,
          records,
          resources,
        );
        // Action audit CIDs do not feed into the proof DAG, but stage
        // CIDs that the action consumed are recorded as inputCids on
        // the audit memento itself for forensic walks.
        const auditInputCids = collectInputCids(action.input, records);
        const resolvedAction = actionRegistry!.resolve<unknown, unknown>(
          action.action,
        );
        if (!resolvedAction) {
          throw new Error(
            `no action registered for capability "${action.action}"`,
          );
        }
        const result: ActionResult<unknown> = await r.runAction(
          resolvedAction,
          resolvedInput,
          auditInputCids,
        );
        resources.set(action.id, result.resource);
      }
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
  resources: Map<string, unknown>,
): unknown {
  if (isReference(input)) {
    return resolveReference(input, workflowInput, records, resources);
  }
  if (Array.isArray(input)) {
    return input.map((item) =>
      resolveInput(item, workflowInput, records, resources),
    );
  }
  if (input && typeof input === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(input as InputObject)) {
      out[k] = resolveInput(v, workflowInput, records, resources);
    }
    return out;
  }
  return input;
}

function resolveReference(
  ref: string,
  workflowInput: unknown,
  records: Map<string, NodeRecord>,
  resources: Map<string, unknown>,
): unknown {
  const parsed = parseReference(ref);
  if (parsed.kind === "input") {
    return walkPath(workflowInput, parsed.path);
  }
  if (parsed.kind === "node") {
    const record = records.get(parsed.nodeId);
    if (!record) {
      throw new Error(
        `reference "${ref}" used before node "${parsed.nodeId}" produced output`,
      );
    }
    if (parsed.field !== "output") {
      throw new Error(
        `reference "${ref}" — only "output" is currently a valid field`,
      );
    }
    return walkPath(record.output, parsed.path);
  }
  // action
  if (!resources.has(parsed.actionId)) {
    throw new Error(
      `reference "${ref}" used before action "${parsed.actionId}" produced its resource`,
    );
  }
  if (parsed.field !== "resource") {
    throw new Error(
      `reference "${ref}" — action references must use .resource`,
    );
  }
  return walkPath(resources.get(parsed.actionId), parsed.path);
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

/**
 * Collect Stage CIDs to thread into the next memento's inputCids.
 *
 * QUIET PART: Action references are deliberately skipped. Action
 * audit CIDs are not part of the proof DAG; only Stage outputs
 * contribute to memento DAG edges. Mixing them in would let the
 * audit DAG bleed into the proof DAG.
 */
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
    // parsed.kind === "action": intentionally skipped; see doc above.
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

// ---------------------------------------------------------------------------
// Kit lockfile
// ---------------------------------------------------------------------------

/**
 * The structure of `.provekit/kits.lock`. Pins each kit to a specific
 * version and content-addressed CID. Same lockfile + same code = same
 * mementos across machines.
 *
 * Spec: docs/specs/2026-04-29-per-language-kit-standard.md
 */
export interface KitLock {
  [kitName: string]: { version: string; cid: string };
}

/**
 * Load `.provekit/kits.lock` relative to projectRoot. Returns null if
 * the file does not exist (most current callers run without it).
 * Throws with a clear message if the file exists but is malformed.
 *
 * The lockfile is not consumed by runManifest in v1 — callers read it
 * to verify their installed kit CIDs match the pinned values before
 * running the workflow.
 */
export function loadKitsLock(projectRoot: string): KitLock | null {
  const path = join(projectRoot, ".provekit", "kits.lock");
  if (!existsSync(path)) return null;
  const text = readFileSync(path, "utf-8");
  let doc: unknown;
  try {
    doc = parseYaml(text);
  } catch (err) {
    throw new Error(
      `.provekit/kits.lock is not valid YAML: ${(err as Error).message}`,
    );
  }
  if (doc === null || doc === undefined) {
    return {};
  }
  if (typeof doc !== "object" || Array.isArray(doc)) {
    throw new Error(
      `.provekit/kits.lock must be a YAML mapping of kit name → { version, cid }`,
    );
  }
  const out: KitLock = {};
  for (const [kitName, raw] of Object.entries(doc as Record<string, unknown>)) {
    if (!raw || typeof raw !== "object") {
      throw new Error(
        `.provekit/kits.lock entry "${kitName}" must be an object with version and cid`,
      );
    }
    const entry = raw as Record<string, unknown>;
    if (typeof entry.version !== "string") {
      throw new Error(
        `.provekit/kits.lock entry "${kitName}".version must be a string`,
      );
    }
    if (typeof entry.cid !== "string") {
      throw new Error(
        `.provekit/kits.lock entry "${kitName}".cid must be a string`,
      );
    }
    out[kitName] = { version: entry.version, cid: entry.cid };
  }
  return out;
}
