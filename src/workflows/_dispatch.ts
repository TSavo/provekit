/**
 * Meta-dispatcher — registry assembly + manifest loading + factory map.
 *
 * Spec: docs/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 *
 * The dispatcher is itself a workflow. cli.ts loads
 * `_dispatch.workflow.yaml`, builds the dispatcher's registries with
 * `registerDispatchRegistries`, and runs the manifest. The dispatcher's
 * three capabilities (parse-argv, locate-workflow, invoke-workflow)
 * resolve via this module.
 *
 * The DISPATCHED workflows — bug-fix, explain, etc. — each ship a
 * companion `<name>.ts` exporting a `register*Registries(deps)` factory
 * + a `load*Manifest()` reader. The dispatcher consumes those through
 * an explicit per-command factory map (see `defaultRegistryFactories`)
 * — explicit registration, not auto-discovery, until the migration
 * stabilizes.
 */

import { readFileSync, readdirSync, existsSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import {
  InMemoryActionRegistry,
  InMemoryRegistry,
  type ActionRegistry,
  type ProducerRegistry,
} from "../workflow/registry.js";
import {
  parseManifest,
  type CliBlock,
  type WorkflowManifest,
} from "../workflow/manifest.js";
import {
  PARSE_ARGV_CAPABILITY,
  makeParseArgvStage,
} from "../workflow/producers/parseArgv.js";
import {
  LOCATE_WORKFLOW_CAPABILITY,
  makeLocateWorkflowStage,
} from "../workflow/producers/locateWorkflow.js";
import {
  INVOKE_WORKFLOW_CAPABILITY,
  makeInvokeWorkflowAction,
  type RegistryFactoryMap,
} from "../workflow/producers/invokeWorkflow.js";
import type { Db } from "../db/index.js";
import type { LLMProvider } from "../fix/types.js";
import type { FixLoopLogger } from "../fix/logger.js";
import { registerBugFixRegistries } from "./bug-fix.js";
import { registerExplainRegistries } from "./explain.js";
import { registerMustRegistries } from "./must.js";
import { registerReevaluateInvariantRegistries } from "./reevaluate-invariant.js";
import { registerDiffRegistries } from "./diff.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/** Default location of the dispatcher manifest, relative to this module. */
const DEFAULT_MANIFEST_PATH = join(__dirname, "_dispatch.workflow.yaml");

export const DISPATCH_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

/** Directory holding all `<name>.workflow.yaml` files. */
export const WORKFLOWS_DIR = __dirname;

export interface DispatchDeps {
  /** The proofkit's local memento DB. */
  db: Db;
  /**
   * LLM provider for invoked workflows that need one. Some workflows
   * (explain) don't; we still pass it through — factories pick what
   * they need.
   */
  llm?: LLMProvider;
  /** Optional logger threaded into invoked workflows. */
  logger?: FixLoopLogger;
  /** Project root threaded into bug-fix / must / etc. */
  projectRoot?: string;
}

export interface DispatchRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/**
 * Construct the dispatcher's own ProducerRegistry + ActionRegistry.
 * These are the registries consulted by `runManifest` when it dispatches
 * the dispatcher manifest's nodes — NOT the registries used by the
 * invoked workflow (those are built per-command by the factory map).
 */
export function registerDispatchRegistries(
  deps: DispatchDeps,
): DispatchRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(PARSE_ARGV_CAPABILITY, makeParseArgvStage());
  registry.register(LOCATE_WORKFLOW_CAPABILITY, makeLocateWorkflowStage());
  actionRegistry.register(
    INVOKE_WORKFLOW_CAPABILITY,
    makeInvokeWorkflowAction({ db: deps.db }),
  );

  return { registry, actionRegistry };
}

/** Read and parse the dispatcher manifest from disk. */
export function loadDispatchManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/**
 * Walk the workflows directory, parse every `*.workflow.yaml`, and
 * return:
 *   - cliBlocks: workflows that declare a `cli:` block, keyed by name
 *     (used by parse-argv to build the per-command arg parser)
 *   - manifestPaths: every workflow's absolute YAML path, keyed by
 *     name (used by locate-workflow to read the chosen one)
 *
 * Underscore-prefixed workflows are skipped from cliBlocks (they are
 * dispatcher internals not addressable as `provekit <command>`) but
 * their paths are also omitted from manifestPaths — they are not
 * dispatchable, so locate-workflow has nothing to look up for them.
 */
export function discoverWorkflows(workflowsDir: string = WORKFLOWS_DIR): {
  cliBlocks: Record<string, CliBlock>;
  manifestPaths: Record<string, string>;
} {
  const cliBlocks: Record<string, CliBlock> = {};
  const manifestPaths: Record<string, string> = {};
  if (!existsSync(workflowsDir)) {
    return { cliBlocks, manifestPaths };
  }
  for (const entry of readdirSync(workflowsDir)) {
    if (!entry.endsWith(".workflow.yaml")) continue;
    const path = join(workflowsDir, entry);
    let manifest: WorkflowManifest;
    try {
      manifest = parseManifest(readFileSync(path, "utf-8"));
    } catch {
      // Malformed YAML in the workflows dir is a developer error;
      // surface elsewhere (parser tests, build). Skipping here keeps
      // dispatcher startup robust to in-flight edits.
      continue;
    }
    if (manifest.name.startsWith("_")) continue;
    manifestPaths[manifest.name] = path;
    if (manifest.cli) cliBlocks[manifest.name] = manifest.cli;
  }
  return { cliBlocks, manifestPaths };
}

/**
 * Default per-command factory map. Each entry maps a workflow name
 * to its `register*Registries(deps)` function. The brief's cut list
 * forbids dynamic auto-discovery in this commit — entries are added
 * here as workflows migrate to the dispatcher.
 *
 * Workflows present in `src/workflows/*.workflow.yaml` but absent
 * from this map will surface as "no registry factory registered"
 * when invoked, until their companion `<name>.ts` is wired in here.
 */
export function defaultRegistryFactories(): RegistryFactoryMap {
  return {
    "bug-fix": (deps: unknown) =>
      registerBugFixRegistries(deps as Parameters<typeof registerBugFixRegistries>[0]),
    explain: (deps: unknown) =>
      registerExplainRegistries(deps as Parameters<typeof registerExplainRegistries>[0]),
    must: (deps: unknown) =>
      registerMustRegistries(deps as Parameters<typeof registerMustRegistries>[0]),
    "reevaluate-invariant": (deps: unknown) =>
      registerReevaluateInvariantRegistries(
        deps as Parameters<typeof registerReevaluateInvariantRegistries>[0],
      ),
    diff: (deps: unknown) =>
      registerDiffRegistries(deps as Parameters<typeof registerDiffRegistries>[0]),
  };
}

/**
 * Workflow input shape consumed by the dispatcher manifest's `$input.*`
 * references.
 */
export interface DispatchWorkflowInput {
  argv: readonly string[];
  cliBlocks: Record<string, CliBlock>;
  manifestPaths: Record<string, string>;
  factories: RegistryFactoryMap;
  deps: unknown;
}
