/**
 * Bug-fix workflow — registry assembly + manifest loading.
 *
 * This is the data-driven replacement for `runFixLoop` in
 * src/fix/orchestrator.ts. The manifest at
 * `src/workflows/bug-fix.workflow.yaml` expresses the pipeline as a DAG;
 * this module wires producer factories to capability names so the
 * manifest's nodes resolve.
 *
 * --- Capability matrix --------------------------------------------------
 * The bug-fix manifest references 10 stage capabilities and 1 action
 * capability. Producer modules for all of them ship under
 * `src/workflow/producers/`. `registerBugFixRegistries` populates a
 * ProducerRegistry with the 10 stages and an ActionRegistry with the
 * single action (open-overlay).
 *
 * Action vs Stage: open-overlay is side-effecting (real git worktree +
 * sqlite handle), so it ships as an Action. Its handle is consumed by
 * do-the-work, generate-complementary, generate-principle-candidate, and
 * bundle via $action.open-overlay.resource references in the YAML.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { Db } from "../db/index.js";
import type { LLMProvider } from "../fix/types.js";
import type { FixLoopLogger } from "../fix/logger.js";
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

import { INTAKE_CAPABILITY, makeIntakeStage } from "../workflow/producers/intake.js";
import {
  INVESTIGATE_CAPABILITY,
  makeInvestigateStage,
} from "../workflow/producers/investigate.js";
import { LOCATE_CAPABILITY, makeLocateStage } from "../workflow/producers/locate.js";
import {
  CLASSIFY_CAPABILITY,
  makeClassifyStage,
} from "../workflow/producers/classify.js";
import {
  RECOGNIZE_CAPABILITY,
  makeRecognizeStage,
} from "../workflow/producers/recognize.js";
import {
  FORMULATE_CAPABILITY,
  makeFormulateStage,
} from "../workflow/producers/formulate.js";
import {
  FORMULATE_VIA_LIFTER_CAPABILITY,
  makeFormulateViaLifterStage,
} from "../workflow/producers/formulateViaLifter.js";
import {
  OPEN_OVERLAY_CAPABILITY,
  makeOpenOverlayAction,
} from "../workflow/producers/openOverlay.js";
import {
  DO_THE_WORK_CAPABILITY,
  makeDoTheWorkStage,
} from "../workflow/producers/doTheWork.js";
import {
  GENERATE_COMPLEMENTARY_CAPABILITY,
  makeGenerateComplementaryStage,
} from "../workflow/producers/generateComplementary.js";
import {
  GENERATE_PRINCIPLE_CANDIDATE_CAPABILITY,
  makeGeneratePrincipleCandidateStage,
} from "../workflow/producers/generatePrincipleCandidate.js";
import { BUNDLE_CAPABILITY, makeBundleStage } from "../workflow/producers/bundle.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/** Default location of the bug-fix manifest, relative to this module. */
const DEFAULT_MANIFEST_PATH = join(__dirname, "bug-fix.workflow.yaml");

export const BUG_FIX_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

/** Stage capabilities the bug-fix manifest references. */
export const BUG_FIX_STAGE_CAPABILITIES = [
  "intake",
  "investigate",
  "locate",
  "classify",
  "recognize",
  "formulate",
  "do-the-work",
  "generate-complementary",
  "generate-principle-candidate",
  "bundle",
] as const;

/** Action capabilities the bug-fix manifest references. */
export const BUG_FIX_ACTION_CAPABILITIES = ["open-overlay"] as const;

/**
 * Combined capability list (stages + actions). Kept for back-compat with
 * the prior shape of this module — callers iterating "every capability
 * the manifest names" still get a single flat list.
 */
export const BUG_FIX_CAPABILITIES = [
  ...BUG_FIX_STAGE_CAPABILITIES,
  ...BUG_FIX_ACTION_CAPABILITIES,
] as const;

/**
 * Capabilities not yet wired. Empty now that all producer modules have
 * been authored and registered. Kept exported as `readonly string[]` so
 * existing callers and tests that iterate it don't break.
 */
export const PENDING_CAPABILITIES: readonly string[] = [];

export interface BugFixDeps {
  db: Db;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Project root used by classify / do-the-work for bp prompt resolution. */
  projectRoot?: string;
}

export interface BugFixRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

/**
 * Construct a registry pre-populated with every bug-fix stage capability.
 * Returns a ProducerRegistry by default for back-compat. To also obtain
 * the ActionRegistry needed for the open-overlay action, call
 * `registerBugFixRegistries`.
 */
export function registerBugFixCapabilities(deps: BugFixDeps): ProducerRegistry {
  return registerBugFixRegistries(deps).registry;
}

/**
 * Construct both the stage ProducerRegistry and the ActionRegistry for
 * the bug-fix workflow, populated with every capability the on-disk
 * manifest references. Pass both to `runManifest` to execute the full
 * YAML.
 */
export function registerBugFixRegistries(deps: BugFixDeps): BugFixRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(INTAKE_CAPABILITY, makeIntakeStage(deps.llm));
  registry.register(
    INVESTIGATE_CAPABILITY,
    makeInvestigateStage({ llm: deps.llm, logger: deps.logger }),
  );
  registry.register(LOCATE_CAPABILITY, makeLocateStage({ db: deps.db }));
  registry.register(
    CLASSIFY_CAPABILITY,
    makeClassifyStage({ llm: deps.llm, projectRoot: deps.projectRoot }),
  );
  registry.register(
    RECOGNIZE_CAPABILITY,
    makeRecognizeStage({ db: deps.db, logger: deps.logger }),
  );
  registry.register(
    FORMULATE_CAPABILITY,
    makeFormulateStage({
      db: deps.db,
      llm: deps.llm,
      logger: deps.logger,
    }),
  );
  // Architecture-correct path. Registered alongside the legacy
  // `formulate` capability; the manifest still references `formulate`
  // for v1, leaving the smoke's module-boundary mock intact. Manifest
  // swap is a follow-up.
  registry.register(
    FORMULATE_VIA_LIFTER_CAPABILITY,
    makeFormulateViaLifterStage({ llm: deps.llm }),
  );
  registry.register(
    DO_THE_WORK_CAPABILITY,
    makeDoTheWorkStage({
      llm: deps.llm,
      logger: deps.logger,
      projectRoot: deps.projectRoot,
    }),
  );
  registry.register(
    GENERATE_COMPLEMENTARY_CAPABILITY,
    makeGenerateComplementaryStage({
      db: deps.db,
      llm: deps.llm,
      logger: deps.logger,
    }),
  );
  registry.register(
    GENERATE_PRINCIPLE_CANDIDATE_CAPABILITY,
    makeGeneratePrincipleCandidateStage({
      db: deps.db,
      llm: deps.llm,
      logger: deps.logger,
    }),
  );
  registry.register(
    BUNDLE_CAPABILITY,
    makeBundleStage({ db: deps.db, logger: deps.logger }),
  );

  actionRegistry.register(
    OPEN_OVERLAY_CAPABILITY,
    makeOpenOverlayAction({ db: deps.db }),
  );

  return { registry, actionRegistry };
}

/**
 * Read and parse the bug-fix manifest from disk. Throws with the parser's
 * native message if the YAML is malformed or fails structural validation.
 */
export function loadBugFixManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape that runManifest expects, encoded for $input refs. */
export interface BugFixWorkflowInput {
  /** Verbatim user-supplied bug report, change request, or property assertion. */
  text: string;
  /** Optional explicit intake adapter name. */
  source?: string;
  /** Optional adapter-specific context (e.g. SAST finding). */
  context?: unknown;
  /**
   * Host project root. Threaded into investigate's prompt + downstream
   * bp-aware stages.
   */
  projectRoot: string;
}
