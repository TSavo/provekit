/**
 * LoadMintSpec Stage — parse a mint specification from a JSON file or
 * compose one from explicit fields.
 *
 * Used as the first Stage of the `mint` workflow. Decouples spec
 * loading (filesystem-coupled) from mint signing (key-coupled,
 * deterministic). The output is a normalized MintSpec the downstream
 * mint-memento Stage signs.
 *
 * Migration of src/cli.mint.ts's per-kind argument handling. The CLI
 * shim normalizes flag-driven invocations (e.g. `mint bridge --source-symbol ...`)
 * into the input shape this Stage accepts; it does NOT itself read
 * argv.
 *
 * Spec: protocol/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import type { Stage } from "../types.js";
import type { EvidenceVariant } from "../../claimEnvelope/types.js";

export const LOAD_MINT_SPEC_CAPABILITY = "load-mint-spec";

export type MintKind = "property" | "bridge" | "catalog" | "generic";

export interface PropertyMintFields {
  bindingHash: string;
  propertyHash: string;
  verdict?: "holds" | "violated" | "decayed" | "undecidable" | "error";
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  evidence?: EvidenceVariant;
  rawWitness?: string;
}

export interface BridgeMintFields {
  sourceSymbol: string;
  sourceLayer: string;
  targetContractCid: string;
  targetLayer: string;
  bindingHash?: string;
  propertyHash?: string;
  producedBy?: string;
  producedAt?: string;
  notes?: string;
}

export interface CatalogMintFields {
  /** Absolute path to the catalog directory. */
  dir: string;
  catalogName?: string;
  catalogVersion?: string;
  producedBy?: string;
  producedAt?: string;
}

export interface GenericMintFields {
  bindingHash: string;
  propertyHash: string;
  verdict?: "holds" | "violated" | "decayed" | "undecidable" | "error";
  producedBy: string;
  producedAt?: string;
  inputCids?: string[];
  evidence: EvidenceVariant;
}

export interface LoadMintSpecInput {
  kind: MintKind;
  /** Optional path to a JSON file containing the spec. */
  specPath?: string;
  /** Optional inline spec object. Wins over specPath when both are supplied. */
  spec?: PropertyMintFields | BridgeMintFields | CatalogMintFields | GenericMintFields;
}

export interface LoadMintSpecOutput {
  kind: MintKind;
  spec: PropertyMintFields | BridgeMintFields | CatalogMintFields | GenericMintFields;
}

export interface MakeLoadMintSpecStageDeps {
  /** Override producer identity. Default: "loadMintSpec@v1". */
  producerVersion?: string;
}

export function makeLoadMintSpecStage(
  deps: MakeLoadMintSpecStageDeps = {},
): Stage<LoadMintSpecInput, LoadMintSpecOutput> {
  const producedBy = deps.producerVersion ?? "loadMintSpec@v1";

  return {
    name: "loadMintSpec",
    producedBy,

    serializeInput(input) {
      const out: Record<string, unknown> = { kind: input.kind };
      if (input.specPath !== undefined) out.specPath = input.specPath;
      if (input.spec !== undefined) out.spec = input.spec;
      return out;
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as LoadMintSpecOutput;
    },

    async run(input) {
      return runLoadMintSpec(input);
    },
  };
}

export async function runLoadMintSpec(
  input: LoadMintSpecInput,
): Promise<LoadMintSpecOutput> {
  if (input.spec !== undefined) {
    return { kind: input.kind, spec: input.spec };
  }
  if (input.specPath === undefined) {
    throw new Error(
      "load-mint-spec requires either spec or specPath; neither was supplied",
    );
  }
  const { readFileSync } = await import("fs");
  const { resolve } = await import("path");
  const text = readFileSync(resolve(input.specPath), "utf-8");
  const parsed = JSON.parse(text);
  return { kind: input.kind, spec: parsed };
}
