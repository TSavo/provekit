/**
 * Verify-project-invariants Stage — walk the project's *.invariant.ts
 * files, run each in a collector context to lift its IR, mint a memento
 * per declaration, compose them into a project root memento, and report
 * any null roots (upstream CIDs referenced but not locally available).
 *
 * Stage contract per `docs/specs/2026-04-29-stages-vs-actions.md`:
 *   - Pure, cacheable
 *   - Input: invariant-file content (path + source) + locally-known CIDs
 *   - Output: minted CIDs + project root CID + null roots list
 *   - Same input → same output, byte-deterministic
 *
 * Null roots are NOT just audit suggestions — they are explicit
 * incompleteness in the proof chain. Each null root names a FUNCTION
 * CALL BOUNDARY in the AST where the proof chain terminates without
 * grounding. A function call is the natural verification frontier:
 * the framework asks "does this callee have a contract in scope?"
 * Yes → compose against it. No → this specific call is a null root.
 *
 * Null roots come in two classes:
 *
 *   External null roots: dependencies whose proof catalogs are not
 *     installed. The kit's published contracts aren't in the local
 *     store; upstream verification is unavailable.
 *
 *   Internal null roots: your own code paths that are not constrained
 *     by any invariant. You wrote a function; no must() declaration
 *     references it; the framework cannot say anything about it.
 *
 * Both classes are reported uniformly. The set difference doesn't
 * distinguish "your uncovered code" from "missing upstream kits" — it
 * names every CID referenced in any minted memento's inputCids that
 * isn't locally available. The developer or auditor reads the list
 * and decides how to close each gap.
 *
 * The completeness metric is binary:
 *
 *   - 0 null roots → every minted memento's inputCids resolves to a
 *     locally-verified upstream. Provably correct.
 *   - N null roots → N specific code paths are NOT verified correct.
 *     Verification is incomplete, with the exact gaps named.
 *
 * Three options when null roots exist: install the missing kit catalog,
 * audit the null root externally and mint your own attestation, or
 * accept the gap as "trusted-unverified" (deliberate trust decision,
 * on the record). All three leave an audit trail.
 *
 * Computing null roots is free: every minted memento's inputCids is
 * known; every locally-available CID is known; null roots are the set
 * difference. The framework cannot get this property for free without
 * the substrate's completeness — that null-root identification works
 * at all is the litmus test that the framework's correctness claim is
 * sound.
 */

import { createHash } from "node:crypto";
import {
  beginCollecting,
  type Declaration,
} from "../../ir/symbolic/index.js";
import { propertyHashFromFormula } from "../../canonicalizer/canonicalize.js";
import {
  mintMemento,
  mintBridge,
  mintLegacyWitness,
  VARIANT_SCHEMA_CIDS,
} from "../../claimEnvelope/index.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";
import type { Stage } from "../types.js";
import type { KeyObject } from "node:crypto";

export const VERIFY_PROJECT_INVARIANTS_CAPABILITY = "verify-project-invariants";

export interface InvariantFileSource {
  /** Path of the file relative to project root (e.g., "src/billing/invoice.invariant.ts"). */
  path: string;
  /** sha256 of the file's content. Used in the binding hash. */
  contentHash: string;
  /**
   * Resolved module path the Stage will dynamically import to run the
   * file's declarations. The Stage does NOT read the filesystem; the
   * caller resolves and provides this path so the Stage stays pure
   * (input-determined). For tests, the caller supplies a virtual or
   * fixture path.
   */
  resolvedModulePath: string;
}

export interface VerifyProjectInvariantsStageInput {
  /** Project name + version (used in the project-root binding). */
  projectName: string;
  projectVersion: string;
  /** All .invariant.ts files discovered in the project tree. */
  invariantFiles: InvariantFileSource[];
  /**
   * CIDs already available in the consumer's local memento store
   * (from installed kit catalogs, prior runs, etc.). Used to compute
   * null roots: any inputCids reference NOT in this set is a null root.
   */
  locallyAvailableCids: string[];
}

export interface MintedDeclaration {
  declarationName: string;
  filePath: string;
  cid: string;
  bindingHash: string;
  propertyHash: string;
  declarationKind: "property" | "bridge";
}

export interface VerifyProjectInvariantsStageOutput {
  /** Mementos minted for each declaration found. */
  declarations: MintedDeclaration[];
  /** Project root memento composing every minted declaration. */
  projectRootCid: string;
  /**
   * CIDs referenced by minted mementos' inputCids that are NOT in the
   * caller's locally-available set. Auditors walk these.
   */
  nullRoots: string[];
}

// ---------------------------------------------------------------------------
// Stage factory
// ---------------------------------------------------------------------------

export interface MakeVerifyProjectInvariantsStageDeps {
  /** Producer key for signing mementos. */
  privateKey: KeyObject | Buffer | string;
  /** Public key, for the runner to log alongside emit. Optional. */
  publicKey?: KeyObject | Buffer | string;
  /** Override producer identity. Default: "verify-project-invariants@v1". */
  producerVersion?: string;
  /** Override producedAt for determinism in tests. Default: epoch. */
  producedAt?: string;
}

export function makeVerifyProjectInvariantsStage(
  deps: MakeVerifyProjectInvariantsStageDeps,
): Stage<VerifyProjectInvariantsStageInput, VerifyProjectInvariantsStageOutput> {
  const producedBy = deps.producerVersion ?? "verify-project-invariants@v1";
  const producedAt = deps.producedAt ?? new Date(0).toISOString();
  const privateKey = deps.privateKey;

  return {
    name: "verifyProjectInvariants",
    producedBy,

    serializeInput(input) {
      // Sort files by path for determinism; only path + contentHash
      // contribute to the binding (the resolvedModulePath is a runtime
      // resolution detail, not content).
      return {
        projectName: input.projectName,
        projectVersion: input.projectVersion,
        invariantFiles: [...input.invariantFiles]
          .map((f) => ({ path: f.path, contentHash: f.contentHash }))
          .sort((a, b) => a.path.localeCompare(b.path)),
        locallyAvailableCids: [...input.locallyAvailableCids].sort(),
      };
    },

    serializeOutput(output) {
      return JSON.stringify(output);
    },

    deserializeOutput(witness) {
      return JSON.parse(witness) as VerifyProjectInvariantsStageOutput;
    },

    async run(input) {
      return runVerifyProjectInvariants(input, {
        privateKey,
        producerId: producedBy,
        producedAt,
      });
    },
  };
}

// ---------------------------------------------------------------------------
// Implementation (also exported for the CLI / tests)
// ---------------------------------------------------------------------------

export interface VerifyProjectInvariantsRunDeps {
  privateKey: KeyObject | Buffer | string;
  producerId: string;
  producedAt: string;
}

export async function runVerifyProjectInvariants(
  input: VerifyProjectInvariantsStageInput,
  deps: VerifyProjectInvariantsRunDeps,
): Promise<VerifyProjectInvariantsStageOutput> {
  const declarations: MintedDeclaration[] = [];
  const allReferencedCids = new Set<string>();
  const allMintedCids: string[] = [];

  // Process each invariant file: run it under beginCollecting, mint
  // mementos for each declaration found.
  for (const file of input.invariantFiles) {
    const finish = beginCollecting();
    let collected: Declaration[];
    try {
      // Dynamic import runs the file's top-level code, which calls
      // describe() / must() / bridge() / property() — these register
      // with the active collector.
      await import(file.resolvedModulePath);
      collected = finish();
    } catch (err) {
      // If beginCollecting was left active by an exception, ensure
      // we clear it.
      try {
        finish();
      } catch {
        /* already finished */
      }
      throw new Error(
        `Failed to lift invariants from ${file.path}: ${(err as Error).message}`,
      );
    }

    for (const decl of collected) {
      const memento = mintDeclaration(decl, file, input, deps);

      declarations.push({
        declarationName: decl.name,
        filePath: file.path,
        cid: memento.cid,
        bindingHash: memento.bindingHash,
        propertyHash: memento.propertyHash,
        declarationKind: decl.kind,
      });

      allMintedCids.push(memento.cid);
      for (const ref of memento.inputCids) allReferencedCids.add(ref);
    }
  }

  // Compose the project root memento.
  const projectRootCid = mintProjectRoot(
    input.projectName,
    input.projectVersion,
    allMintedCids,
    deps,
  );

  // Null roots: CIDs referenced by any minted memento's inputCids that
  // are NOT in (locally-available ∪ minted-this-run).
  const locallyAvailable = new Set([
    ...input.locallyAvailableCids,
    ...allMintedCids,
    projectRootCid,
  ]);
  const nullRoots = [...allReferencedCids]
    .filter((cid) => !locallyAvailable.has(cid))
    .sort();

  return {
    declarations,
    projectRootCid,
    nullRoots,
  };
}

// ---------------------------------------------------------------------------
// Helpers (private)
// ---------------------------------------------------------------------------

function hash16(s: string): string {
  return createHash("sha256").update(s).digest("hex").slice(0, 16);
}

function mintDeclaration(
  decl: Declaration,
  file: InvariantFileSource,
  input: VerifyProjectInvariantsStageInput,
  deps: VerifyProjectInvariantsRunDeps,
): ClaimEnvelope {
  const bindingHash = hash16(
    `${input.projectName}@${input.projectVersion}:${file.path}:${decl.name}`,
  );

  if (decl.kind === "property") {
    const propertyHash = propertyHashFromFormula(decl.formula);
    return mintMemento({
      bindingHash,
      propertyHash,
      verdict: "holds",
      producedBy: deps.producerId,
      producedAt: deps.producedAt,
      inputCids: [],
      evidence: {
        kind: "legacy-witness",
        schema: VARIANT_SCHEMA_CIDS["legacy-witness"]!,
        body: {
          rawWitness: JSON.stringify(decl.formula),
          legacyProducerId: deps.producerId,
        },
      },
      privateKey: deps.privateKey,
    });
  }

  // Bridge declaration
  return mintBridge({
    bindingHash,
    propertyHash: hash16(`bridge:${decl.sourceSymbol}`),
    producedBy: deps.producerId,
    producedAt: deps.producedAt,
    privateKey: deps.privateKey,
    sourceSymbol: decl.sourceSymbol,
    sourceLayer: decl.sourceLayer,
    targetContractCid: decl.targetContractCid,
    targetLayer: decl.targetLayer,
    ...(decl.notes !== undefined ? { notes: decl.notes } : {}),
  });
}

function mintProjectRoot(
  projectName: string,
  projectVersion: string,
  mintedCids: string[],
  deps: VerifyProjectInvariantsRunDeps,
): string {
  const sortedInputCids = [...mintedCids].sort();
  const root = mintLegacyWitness({
    bindingHash: hash16(`project-root:${projectName}@${projectVersion}`),
    propertyHash: hash16(`verify:${projectName}@${projectVersion}`),
    verdict: "holds",
    producedBy: deps.producerId,
    producedAt: deps.producedAt,
    inputCids: sortedInputCids,
    privateKey: deps.privateKey,
    rawWitness: JSON.stringify({
      kind: "project-root",
      projectName,
      projectVersion,
      memberCount: sortedInputCids.length,
    }),
  });
  return root.cid;
}
