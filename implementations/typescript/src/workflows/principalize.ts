/**
 * Principalize workflow — registry assembly + manifest loading.
 *
 * The principalize workflow lifts a recurring invariant pattern into a
 * candidate principle. The manifest at
 * `src/workflows/principalize.workflow.yaml` expresses the pipeline as a
 * DAG; this module wires producer factories to capability names so the
 * manifest's nodes resolve.
 *
 * --- Capability matrix --------------------------------------------------
 * Three Stage capabilities + one Action capability. Producer modules ship
 * under `src/workflow/producers/`. `registerPrincipalizeRegistries`
 * populates a ProducerRegistry with the three Stages and an
 * ActionRegistry with the one Action (publish-principle).
 *
 * Stage vs Action: publish-principle writes to
 * `.provekit/principles/<id>.json` — a shared resource the rest of the
 * framework reads — so it ships as an Action. The audit memento records
 * the principle name + filesystem path so a forensic walk can reach the
 * published file.
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
  COLLECT_INVARIANTS_CAPABILITY,
  makeCollectInvariantsStage,
} from "../workflow/producers/collectInvariants.js";
import {
  CLUSTER_BY_SHAPE_CAPABILITY,
  makeClusterByShapeStage,
} from "../workflow/producers/clusterByShape.js";
import {
  VALIDATE_ADVERSARIAL_CAPABILITY,
  makeValidateAdversarialStage,
} from "../workflow/producers/validateAdversarial.js";
import {
  PUBLISH_PRINCIPLE_CAPABILITY,
  makePublishPrincipleAction,
} from "../workflow/producers/publishPrinciple.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "principalize.workflow.yaml");

export const PRINCIPALIZE_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const PRINCIPALIZE_STAGE_CAPABILITIES = [
  "collect-invariants",
  "cluster-by-shape",
  "validate-adversarial",
] as const;

export const PRINCIPALIZE_ACTION_CAPABILITIES = ["publish-principle"] as const;

export const PRINCIPALIZE_CAPABILITIES = [
  ...PRINCIPALIZE_STAGE_CAPABILITIES,
  ...PRINCIPALIZE_ACTION_CAPABILITIES,
] as const;

export interface PrincipalizeRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/**
 * Construct both the stage ProducerRegistry and the ActionRegistry for
 * the principalize workflow. Pass both to `runManifest` to execute the
 * full YAML.
 *
 * No deps argument: every Stage in this workflow is pure-data; the
 * Action's only dep is the host filesystem (resolved per-invocation via
 * the input's projectRoot). When a future producer needs an LLM or DB
 * for cross-codebase validation, this signature gains a deps parameter
 * — the existing capability registrations stay shaped the same.
 */
export function registerPrincipalizeRegistries(): PrincipalizeRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(
    COLLECT_INVARIANTS_CAPABILITY,
    makeCollectInvariantsStage(),
  );
  registry.register(CLUSTER_BY_SHAPE_CAPABILITY, makeClusterByShapeStage());
  registry.register(
    VALIDATE_ADVERSARIAL_CAPABILITY,
    makeValidateAdversarialStage(),
  );

  actionRegistry.register(
    PUBLISH_PRINCIPLE_CAPABILITY,
    makePublishPrincipleAction(),
  );

  return { registry, actionRegistry };
}

/**
 * Read and parse the principalize manifest from disk. Throws with the
 * parser's native message if the YAML is malformed or fails structural
 * validation.
 */
export function loadPrincipalizeManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape encoded for $input refs in the manifest. */
export interface PrincipalizeWorkflowInput {
  /** Host project root containing `.provekit/invariants/`. */
  projectRoot: string;
  /**
   * Whitelist of invariant ids to consider. Optional — when omitted,
   * the entire local invariant store is the corpus.
   */
  invariantCorpus?: string[];
  /** Filename root for the published principle. */
  proposedPrincipleName: string;
  /** Bug-class slug — often equals proposedPrincipleName. */
  proposedBugClassId: string;
  /** Optional human-friendly description; auto-generated when omitted. */
  proposedDescription?: string;
  /** Optional confidence label. Defaults to "medium". */
  proposedConfidence?: "high" | "medium" | "low";
}
