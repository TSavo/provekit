/**
 * Migrate workflow — registry assembly + manifest loading.
 *
 * The migrate workflow surfaces a punch list when a kit's proofHash
 * bumps. The manifest at `src/workflows/migrate.workflow.yaml`
 * expresses the pipeline as a DAG; this module wires producer
 * factories to capability names so the manifest's nodes resolve.
 *
 * --- Capability matrix --------------------------------------------------
 * Three Stage capabilities (load-catalog used twice — once for old,
 * once for new) + one Action capability. Producer modules ship under
 * `src/workflow/producers/`. `registerMigrateRegistries` populates a
 * ProducerRegistry with the three Stages and an ActionRegistry with
 * the one Action (mint-migration-plan).
 *
 * Stage vs Action: mint-migration-plan writes to
 * `.provekit/migrations/<old>-to-<new>.md` — a shared resource — so
 * it ships as an Action. The audit memento records the path + counts.
 */

import { readFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { Db } from "../db/index.js";
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
  LOAD_CATALOG_CAPABILITY,
  makeLoadCatalogStage,
} from "../workflow/producers/loadCatalog.js";
import {
  DIFF_CATALOGS_CAPABILITY,
  makeDiffCatalogsStage,
} from "../workflow/producers/diffCatalogs.js";
import {
  FIND_IMPACTED_CALLSITES_CAPABILITY,
  makeFindImpactedCallsitesStage,
} from "../workflow/producers/findImpactedCallsites.js";
import {
  MINT_MIGRATION_PLAN_CAPABILITY,
  makeMintMigrationPlanAction,
} from "../workflow/producers/mintMigrationPlan.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const DEFAULT_MANIFEST_PATH = join(__dirname, "migrate.workflow.yaml");

export const MIGRATE_MANIFEST_PATH = DEFAULT_MANIFEST_PATH;

export const MIGRATE_STAGE_CAPABILITIES = [
  "load-catalog",
  "diff-catalogs",
  "find-impacted-callsites",
] as const;

export const MIGRATE_ACTION_CAPABILITIES = ["mint-migration-plan"] as const;

export const MIGRATE_CAPABILITIES = [
  ...MIGRATE_STAGE_CAPABILITIES,
  ...MIGRATE_ACTION_CAPABILITIES,
] as const;

export interface MigrateDeps {
  /**
   * Memento store the load-catalog stage reads from. The migrate
   * workflow's discipline is "leaves and roots, not walks" — it does
   * NOT fetch from a registry; the caller's local DB is the only
   * source.
   */
  db: Db;
}

export interface MigrateRegistries {
  registry: ProducerRegistry;
  actionRegistry: ActionRegistry;
}

export function registerMigrateRegistries(
  deps: MigrateDeps,
): MigrateRegistries {
  const registry = new InMemoryRegistry();
  const actionRegistry = new InMemoryActionRegistry();

  registry.register(
    LOAD_CATALOG_CAPABILITY,
    makeLoadCatalogStage({ db: deps.db }),
  );
  registry.register(DIFF_CATALOGS_CAPABILITY, makeDiffCatalogsStage());
  registry.register(
    FIND_IMPACTED_CALLSITES_CAPABILITY,
    makeFindImpactedCallsitesStage(),
  );

  actionRegistry.register(
    MINT_MIGRATION_PLAN_CAPABILITY,
    makeMintMigrationPlanAction(),
  );

  return { registry, actionRegistry };
}

export function loadMigrateManifest(
  manifestPath: string = DEFAULT_MANIFEST_PATH,
): WorkflowManifest {
  const yaml = readFileSync(manifestPath, "utf-8");
  return parseManifest(yaml);
}

/** Workflow input shape encoded for $input refs in the manifest. */
export interface MigrateWorkflowInput {
  /** Host project root containing `.provekit/invariants/`. */
  projectRoot: string;
  /** The proofHash currently in use. */
  oldProofHash: string;
  /** The proposed new proofHash. */
  newProofHash: string;
}
