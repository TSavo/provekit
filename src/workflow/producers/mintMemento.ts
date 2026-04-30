/**
 * MintMemento Stage — sign a memento envelope from a normalized spec.
 *
 * The framework's CORE primitive operation, exposed as a workflow Stage.
 * Given a kind + spec + key, deterministically produces a signed
 * ClaimEnvelope. Same input → same memento (the key is part of the
 * input). Stage caching means re-running the same mint reuses the
 * cached envelope; the substrate's content-addressed CID is the cache
 * key.
 *
 * Key handling: the Stage takes the key material directly (PEM string
 * or KeyObject). Key resolution (--key path → PEM, $PROVEKIT_KEY env
 * var, ephemeral generation) is the CLI shim's responsibility — keeping
 * filesystem and env access out of the Stage. Ephemeral generation in
 * particular is non-deterministic and has to live outside any cacheable
 * Stage.
 *
 * Migration of the four mint subcommands (property/bridge/catalog/generic)
 * in src/cli.mint.ts.
 *
 * Spec: docs/specs/2026-04-29-correctness-is-a-hash.md
 *       §"All operations are YAML workflows"
 */

import { createPrivateKey, createPublicKey, createHash } from "node:crypto";
import type { KeyObject } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { resolve as resolvePath, join } from "node:path";
import {
  mintMemento as mintMementoEnvelope,
  mintBridge as mintBridgeEnvelope,
  mintLegacyWitness,
  VARIANT_SCHEMA_CIDS,
} from "../../claimEnvelope/index.js";
import type { ClaimEnvelope, EvidenceVariant } from "../../claimEnvelope/types.js";
import type { Stage } from "../types.js";
import type {
  MintKind,
  LoadMintSpecOutput,
  PropertyMintFields,
  BridgeMintFields,
  CatalogMintFields,
  GenericMintFields,
} from "./loadMintSpec.js";

export const MINT_MEMENTO_CAPABILITY = "mint-memento";

export interface MintMementoInput {
  /** The loaded spec (typically threaded from load-mint-spec.output). */
  loaded: LoadMintSpecOutput;
  /** ed25519 private key in PEM format. The CLI shim resolves --key, $PROVEKIT_KEY, or ephemeral. */
  privateKeyPem: string;
}

export interface MintMementoOutput {
  envelope: ClaimEnvelope;
  /** Public key fingerprint (sha256 of SPKI DER, hex). For audit + ephemeral verification. */
  publicKeyFingerprint: string;
}

export interface MakeMintMementoStageDeps {
  /** Override producer identity. Default: "mintMemento@v1". */
  producerVersion?: string;
}

export function makeMintMementoStage(
  deps: MakeMintMementoStageDeps = {},
): Stage<MintMementoInput, MintMementoOutput> {
  const producedBy = deps.producerVersion ?? "mintMemento@v1";

  return {
    name: "mintMemento",
    producedBy,

    serializeInput(input) {
      // Hash the key fingerprint, not the key bytes. Two invocations
      // with the same key produce the same memento (the underlying
      // signEnvelope is deterministic for ed25519); two invocations
      // with different keys produce different mementos. The
      // fingerprint suffices for cache-key purposes and avoids
      // putting raw key material into the substrate.
      const privateKey = createPrivateKey({ key: input.privateKeyPem, format: "pem" });
      const publicKey = createPublicKey(privateKey);
      const fingerprint = publicKeyFingerprint(publicKey);
      return {
        loaded: input.loaded,
        publicKeyFingerprint: fingerprint,
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as MintMementoOutput;
    },

    async run(input) {
      return runMintMemento(input);
    },
  };
}

export async function runMintMemento(
  input: MintMementoInput,
): Promise<MintMementoOutput> {
  const privateKey = createPrivateKey({ key: input.privateKeyPem, format: "pem" });
  const publicKey = createPublicKey(privateKey);
  const fingerprint = publicKeyFingerprint(publicKey);

  const envelope = await mintByKind(input.loaded, privateKey);
  return { envelope, publicKeyFingerprint: fingerprint };
}

async function mintByKind(
  loaded: LoadMintSpecOutput,
  privateKey: KeyObject,
): Promise<ClaimEnvelope> {
  switch (loaded.kind) {
    case "property":
      return mintPropertyEnvelope(loaded.spec as PropertyMintFields, privateKey);
    case "bridge":
      return mintBridgeFlow(loaded.spec as BridgeMintFields, privateKey);
    case "catalog":
      return mintCatalogEnvelope(loaded.spec as CatalogMintFields, privateKey);
    case "generic":
      return mintGenericEnvelope(loaded.spec as GenericMintFields, privateKey);
    default: {
      const exhaustive: never = loaded.kind;
      throw new Error(`unknown mint kind: ${exhaustive}`);
    }
  }
}

function mintPropertyEnvelope(
  spec: PropertyMintFields,
  privateKey: KeyObject,
): ClaimEnvelope {
  const evidence: EvidenceVariant = spec.evidence ?? {
    kind: "legacy-witness",
    schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
    body: {
      rawWitness: spec.rawWitness ?? "{}",
      legacyProducerId: spec.producedBy,
    },
  };
  return mintMementoEnvelope({
    bindingHash: spec.bindingHash,
    propertyHash: spec.propertyHash,
    verdict: spec.verdict ?? "holds",
    producedBy: spec.producedBy,
    ...(spec.producedAt !== undefined ? { producedAt: spec.producedAt } : {}),
    inputCids: spec.inputCids ?? [],
    evidence,
    privateKey,
  });
}

function mintBridgeFlow(
  spec: BridgeMintFields,
  privateKey: KeyObject,
): ClaimEnvelope {
  const producedBy = spec.producedBy ?? `${spec.sourceLayer}@cli`;
  const bindingHash = spec.bindingHash ?? hash16(`${spec.sourceLayer}:${spec.sourceSymbol}`);
  const propertyHash = spec.propertyHash ?? hash16(`bridge:${spec.sourceSymbol}`);
  return mintBridgeEnvelope({
    bindingHash,
    propertyHash,
    producedBy,
    ...(spec.producedAt !== undefined ? { producedAt: spec.producedAt } : {}),
    privateKey,
    sourceSymbol: spec.sourceSymbol,
    sourceLayer: spec.sourceLayer,
    targetContractCid: spec.targetContractCid,
    targetLayer: spec.targetLayer,
    ...(spec.notes !== undefined ? { notes: spec.notes } : {}),
  });
}

function mintCatalogEnvelope(
  spec: CatalogMintFields,
  privateKey: KeyObject,
): ClaimEnvelope {
  const dir = resolvePath(spec.dir);
  const files = readdirSync(dir)
    .filter((f) => f.endsWith(".json") && f !== "catalog.json")
    .sort();

  const cids: string[] = [];
  for (const f of files) {
    let parsed: { cid?: unknown; bindingHash?: unknown };
    try {
      parsed = JSON.parse(readFileSync(join(dir, f), "utf-8"));
    } catch {
      continue;
    }
    if (typeof parsed.cid === "string" && typeof parsed.bindingHash === "string") {
      cids.push(parsed.cid);
    }
  }
  cids.sort();

  const catalogName = spec.catalogName ?? "catalog";
  const catalogVersion = spec.catalogVersion ?? "0.0.1";
  const producedBy = spec.producedBy ?? `${catalogName}@${catalogVersion}`;

  return mintLegacyWitness({
    bindingHash: hash16(`${catalogName}@${catalogVersion}`),
    propertyHash: hash16(`catalog-root:${catalogName}@${catalogVersion}`),
    verdict: "holds",
    producedBy,
    ...(spec.producedAt !== undefined ? { producedAt: spec.producedAt } : {}),
    inputCids: cids,
    privateKey,
    rawWitness: JSON.stringify({
      kind: "catalog",
      name: catalogName,
      version: catalogVersion,
      memberCount: cids.length,
      members: cids,
    }),
  });
}

function mintGenericEnvelope(
  spec: GenericMintFields,
  privateKey: KeyObject,
): ClaimEnvelope {
  return mintMementoEnvelope({
    bindingHash: spec.bindingHash,
    propertyHash: spec.propertyHash,
    verdict: spec.verdict ?? "holds",
    producedBy: spec.producedBy,
    ...(spec.producedAt !== undefined ? { producedAt: spec.producedAt } : {}),
    inputCids: spec.inputCids ?? [],
    evidence: spec.evidence,
    privateKey,
  });
}

function publicKeyFingerprint(publicKey: KeyObject): string {
  const der = publicKey.export({ type: "spki", format: "der" }) as Buffer;
  return createHash("sha256").update(der).digest("hex");
}

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

void (mintBridgeFlow as (s: BridgeMintFields, k: KeyObject) => ClaimEnvelope);

// Re-export MintKind for convenience.
export type { MintKind };
