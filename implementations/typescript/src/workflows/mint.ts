/**
 * Mint workflow — registry assembly + manifest loading.
 *
 * `provekit mint <kind> [--spec <path>] [--key <path>] [--out <path>]`
 * signs a memento envelope from a JSON specification. Two Stages
 * (load-mint-spec → mint-memento) plus one Action (write-memento-file).
 *
 * Stdout-only mode (the cli.mint.ts default when --out is omitted) is
 * NOT this workflow's surface. The CLI shim either runs the Stages
 * directly and pipes JSON to stdout, or runs the full workflow when
 * --out is supplied.
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
  LOAD_MINT_SPEC_CAPABILITY,
  makeLoadMintSpecStage,
} from "../workflow/producers/loadMintSpec.js";
import {
  MINT_MEMENTO_CAPABILITY,
  makeMintMementoStage,
} from "../workflow/producers/mintMemento.js";
import {
  WRITE_MEMENTO_FILE_CAPABILITY,
  makeWriteMementoFileAction,
} from "../workflow/producers/writeMementoFile.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "mint.workflow.yaml");

export const MINT_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const MINT_STAGE_CAPABILITIES = [
  LOAD_MINT_SPEC_CAPABILITY,
  MINT_MEMENTO_CAPABILITY,
] as const;
export const MINT_ACTION_CAPABILITIES = [
  WRITE_MEMENTO_FILE_CAPABILITY,
] as const;
export const MINT_CAPABILITIES = [
  ...MINT_STAGE_CAPABILITIES,
  ...MINT_ACTION_CAPABILITIES,
] as const;

export interface MintDeps {}

export interface MintRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerMintRegistries(_deps: MintDeps = {}): MintRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(LOAD_MINT_SPEC_CAPABILITY, makeLoadMintSpecStage());
  registry.register(MINT_MEMENTO_CAPABILITY, makeMintMementoStage());

  actionRegistry.register(
    WRITE_MEMENTO_FILE_CAPABILITY,
    makeWriteMementoFileAction(),
  );

  return { registry, actionRegistry };
}

export function loadMintManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

export interface MintWorkflowInput {
  kind: "property" | "bridge" | "catalog" | "generic";
  /** Optional JSON spec file path; ignored when spec is supplied. */
  specPath?: string;
  /** Optional inline spec; wins over specPath. */
  spec?: unknown;
  /** ed25519 private key, PEM encoded. */
  privateKeyPem: string;
  /** Absolute path the envelope is written to. */
  outPath: string;
}
