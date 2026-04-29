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
 * The bug-fix manifest references 11 capabilities. Producers for 7 of
 * them already exist under `src/workflow/producers/`; the remaining 4
 * (recognize, open-overlay, generate-complementary,
 * generate-principle-candidate) are being authored by a parallel agent.
 * Until those producer modules land, `registerBugFixCapabilities` only
 * registers the 7 it can construct. Calling `runManifest` against the
 * full manifest will throw the runner's "capability X not registered"
 * error when topo execution hits one of the missing nodes — that's
 * intentional and surfaces the gap. The manifest itself parses + validates
 * fine because validation is structural and doesn't consult the registry.
 *
 * Drop-in for the parallel agent: when the four producer modules
 * land, add the corresponding `registry.register(<CAP>, make<Stage>(deps))`
 * calls below — no other change is required.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { Db } from "../db/index.js";
import type { LLMProvider } from "../fix/types.js";
import type { FixLoopLogger } from "../fix/logger.js";
import { InMemoryRegistry, type ProducerRegistry } from "../workflow/registry.js";
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
  FORMULATE_CAPABILITY,
  makeFormulateStage,
} from "../workflow/producers/formulate.js";
import {
  DO_THE_WORK_CAPABILITY,
  makeDoTheWorkStage,
} from "../workflow/producers/doTheWork.js";
import { BUNDLE_CAPABILITY, makeBundleStage } from "../workflow/producers/bundle.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

/** Default location of the bug-fix manifest, relative to this module. */
const DEFAULT_MANIFEST_PATH = join(__dirname, "bug-fix.workflow.yaml");

export const BUG_FIX_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

/** Capabilities the bug-fix manifest references. */
export const BUG_FIX_CAPABILITIES = [
  "intake",
  "investigate",
  "locate",
  "classify",
  "recognize",
  "formulate",
  "open-overlay",
  "do-the-work",
  "generate-complementary",
  "generate-principle-candidate",
  "bundle",
] as const;

/**
 * Capabilities not yet wired in this branch (their producer modules are
 * authored by a parallel agent). Surfaces the gap to callers in tests
 * and CLI flows so it can't be silently masked.
 */
export const PENDING_CAPABILITIES = [
  "recognize",
  "open-overlay",
  "generate-complementary",
  "generate-principle-candidate",
] as const;

export interface BugFixDeps {
  db: Db;
  llm: LLMProvider;
  logger?: FixLoopLogger;
  /** Project root used by classify / do-the-work for bp prompt resolution. */
  projectRoot?: string;
}

/**
 * Construct a registry pre-populated with every bug-fix capability whose
 * producer module exists on this branch. See PENDING_CAPABILITIES for the
 * gap. The function does not throw on missing producers — it silently
 * leaves them unregistered so that downstream `runManifest` produces a
 * clear "capability X not registered" error at execution time.
 */
export function registerBugFixCapabilities(deps: BugFixDeps): ProducerRegistry {
  const registry = new InMemoryRegistry();

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
    FORMULATE_CAPABILITY,
    makeFormulateStage({
      db: deps.db,
      llm: deps.llm,
      logger: deps.logger,
    }),
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
    BUNDLE_CAPABILITY,
    makeBundleStage({ db: deps.db, logger: deps.logger }),
  );

  // The four pending capabilities (recognize, open-overlay,
  // generate-complementary, generate-principle-candidate) are intentionally
  // not registered. When their producer modules land, add the
  // registry.register(...) calls here.

  return registry;
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
