/**
 * Lint workflow — registry assembly + manifest loading.
 *
 * `provekit lint [projectRoot]` runs the principle library across the
 * project. Single Stage that wraps the existing principle-library scan
 * (formerly src/cli.ts:runLint) without behavioral changes — the
 * migration is imperative→declarative, not a rewrite.
 *
 * No Actions: the only side effect is the scratch SQLite DB the Stage
 * cleans up itself.
 */

import { readFileSync } from "fs";
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
  type WorkflowManifest,
} from "../workflow/manifest.js";
import {
  RUN_PRINCIPLE_LIBRARY_LINT_CAPABILITY,
  makeRunPrincipleLibraryLintStage,
} from "../workflow/producers/runPrincipleLibraryLint.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "lint.workflow.yaml");

export const LINT_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const LINT_STAGE_CAPABILITIES = [
  RUN_PRINCIPLE_LIBRARY_LINT_CAPABILITY,
] as const;
export const LINT_ACTION_CAPABILITIES = [] as const;
export const LINT_CAPABILITIES = [
  ...LINT_STAGE_CAPABILITIES,
  ...LINT_ACTION_CAPABILITIES,
] as const;

export interface LintDeps {
  // No deps; the Stage opens its own scratch DB.
}

export interface LintRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerLintRegistries(
  _deps: LintDeps = {},
): LintRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(
    RUN_PRINCIPLE_LIBRARY_LINT_CAPABILITY,
    makeRunPrincipleLibraryLintStage(),
  );

  return { registry, actionRegistry };
}

export function loadLintManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

export interface LintWorkflowInput {
  projectRoot: string;
  principlesDir: string;
  drizzleFolder: string;
  verbose: boolean;
}
