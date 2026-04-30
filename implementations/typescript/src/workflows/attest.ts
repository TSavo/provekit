/**
 * Attest workflow — registry assembly + manifest loading.
 *
 * `provekit attest [projectRoot] [--key <path>] [--out <dir>] [--ci]`
 * walks the project's .invariant.ts files, mints a memento per
 * declaration, composes a project root, and identifies null roots.
 *
 * Two Stages and one Action:
 *   - scan-invariant-files       (NEW) — filesystem walk
 *   - verify-project-invariants  (existing) — minting + null-root analysis
 *   - write-attest-summary       (NEW Action) — persist the summary JSON
 *
 * Key resolution + ephemeral generation stay in the CLI shim and are
 * passed in as `privateKey`. Same pattern as the mint workflow.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { KeyObject } from "node:crypto";
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
  SCAN_INVARIANT_FILES_CAPABILITY,
  makeScanInvariantFilesStage,
} from "../workflow/producers/scanInvariantFiles.js";
import {
  VERIFY_PROJECT_INVARIANTS_CAPABILITY,
  makeVerifyProjectInvariantsStage,
} from "../workflow/producers/verifyProjectInvariants.js";
import {
  WRITE_ATTEST_SUMMARY_CAPABILITY,
  makeWriteAttestSummaryAction,
} from "../workflow/producers/writeAttestSummary.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "attest.workflow.yaml");

export const ATTEST_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const ATTEST_STAGE_CAPABILITIES = [
  SCAN_INVARIANT_FILES_CAPABILITY,
  VERIFY_PROJECT_INVARIANTS_CAPABILITY,
] as const;
export const ATTEST_ACTION_CAPABILITIES = [
  WRITE_ATTEST_SUMMARY_CAPABILITY,
] as const;
export const ATTEST_CAPABILITIES = [
  ...ATTEST_STAGE_CAPABILITIES,
  ...ATTEST_ACTION_CAPABILITIES,
] as const;

export interface AttestDeps {
  /** ed25519 private key (resolved by the CLI shim from --key, env, or ephemeral). */
  privateKey: KeyObject | Buffer | string;
  /** Producer identity for verify-project-invariants. Default: "attest@<projectName>". */
  producerVersion?: string;
  /** Override producedAt for deterministic tests. */
  producedAt?: string;
}

export interface AttestRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerAttestRegistries(deps: AttestDeps): AttestRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(
    SCAN_INVARIANT_FILES_CAPABILITY,
    makeScanInvariantFilesStage(),
  );
  registry.register(
    VERIFY_PROJECT_INVARIANTS_CAPABILITY,
    makeVerifyProjectInvariantsStage({
      privateKey: deps.privateKey,
      ...(deps.producerVersion !== undefined
        ? { producerVersion: deps.producerVersion }
        : {}),
      ...(deps.producedAt !== undefined ? { producedAt: deps.producedAt } : {}),
    }),
  );

  actionRegistry.register(
    WRITE_ATTEST_SUMMARY_CAPABILITY,
    makeWriteAttestSummaryAction(),
  );

  return { registry, actionRegistry };
}

export function loadAttestManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

export interface AttestWorkflowInput {
  scanRoot: string;
  projectRoot: string;
  projectName: string;
  projectVersion: string;
  /**
   * CIDs already available in the consumer's local memento store.
   * Empty in v1 — every bridge target becomes a null root.
   */
  locallyAvailableCids: string[];
  /** Absolute path the attest summary JSON is written to. */
  outPath: string;
}
