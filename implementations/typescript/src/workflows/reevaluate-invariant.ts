/**
 * Reevaluate-invariant workflow — registry assembly + manifest loading.
 *
 * `provekit reevaluate-invariant <id>` asks the LLM whether a decayed
 * invariant survives the edit applied to its target function, and
 * recommends the next workflow (rebind / strengthen / weaken / refute /
 * retire).
 *
 * Three Stages:
 *   - load-invariant (read .provekit/invariants/<id>.json)
 *   - locate-current-function (substrate hash search + body readback)
 *   - reevaluate-invariant (LLM judgment, structured verdict)
 *
 * Spec: protocol/specs/2026-04-29-the-semantic-envelope.md (case 4 routing
 * for the binding state machine).
 */

import { dirname, join } from "path";
import { readFileSync } from "fs";
import { fileURLToPath } from "url";
import type { LLMProvider } from "../fix/types.js";
import {
  InMemoryRegistry,
  type ProducerRegistry,
} from "../workflow/registry.js";
import { parseManifest, type WorkflowManifest } from "../workflow/manifest.js";
import {
  LOAD_INVARIANT_CAPABILITY,
  makeLoadInvariantStage,
} from "../workflow/producers/loadInvariant.js";
import {
  LOCATE_CURRENT_FUNCTION_CAPABILITY,
  makeLocateCurrentFunctionStage,
} from "../workflow/producers/locateCurrentFunction.js";
import {
  REEVALUATE_INVARIANT_CAPABILITY,
  makeReevaluateInvariantStage,
} from "../workflow/producers/reevaluateInvariant.js";

const __dirname = dirname(fileURLToPath(import.meta.url));

export interface ReevaluateInvariantWorkflowDeps {
  llm: LLMProvider;
}

export interface ReevaluateInvariantRegistries {
  registry: ProducerRegistry;
}

export function registerReevaluateInvariantRegistries(
  deps: ReevaluateInvariantWorkflowDeps,
): ReevaluateInvariantRegistries {
  const registry = new InMemoryRegistry();
  registry.register(LOAD_INVARIANT_CAPABILITY, makeLoadInvariantStage());
  registry.register(LOCATE_CURRENT_FUNCTION_CAPABILITY, makeLocateCurrentFunctionStage());
  registry.register(
    REEVALUATE_INVARIANT_CAPABILITY,
    makeReevaluateInvariantStage({ llm: deps.llm }),
  );
  return { registry };
}

export function loadReevaluateInvariantManifest(): WorkflowManifest {
  const yaml = readFileSync(
    join(__dirname, "reevaluate-invariant.workflow.yaml"),
    "utf-8",
  );
  return parseManifest(yaml);
}
