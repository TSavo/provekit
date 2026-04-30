/**
 * Stage 5 of the prove-lift pipeline (STUB).
 *
 * Real Mint composes the accepted candidate's body into the fixed
 * forall scaffold, hands the resulting surface text to the existing
 * src/ir/lift/ TS-IR lifter to produce an IrFormula, runs the
 * canonicalizer for the propertyHash, mints property + bridge
 * mementos via src/claimEnvelope/mint.ts, and bundles them into a
 * .proof file via src/proofEnvelope/index.ts::buildProofEnvelope.
 *
 * v0 (this run) ships a typed stub that throws an explicit
 * "not-yet-implemented" error so calling code surfaces a loud failure
 * instead of silently producing nothing. Run-2 wires the real chain.
 */

import type { FunctionShape } from "./detect.js";
import type { Candidate } from "./propose.js";

export interface MintInput {
  shape: FunctionShape;
  accepted: Candidate[];
  /** Path of the .proof file to write. Defaults to <input>.proof. */
  outPath?: string;
  /** PEM-encoded ed25519 private key. Falls back to env / ephemeral. */
  privateKeyPem?: string;
}

export interface MintResult {
  proofPath: string;
  proofCid: string;
  propertyHash: string;
  bindingHash: string;
}

export async function mint(_input: MintInput): Promise<MintResult> {
  // Run-2 wiring outline (intentionally not implemented in v0):
  //
  //   1. Compose surface text:
  //        const surface = `import { property, forAll } from "provekit/ir";\n` +
  //                        `property("${shape.name}_lifted", ` +
  //                          `forAll<${binderSort}>(${binder} => ${candidate.body}));`;
  //
  //   2. Build an in-memory ts.Program with that surface as a
  //      .invariant.ts virtual file. (See cross-lang-end-to-end.test.ts
  //      for the existing pattern.)
  //
  //   3. Call liftProject(program) from src/ir/lift/index.ts and pull
  //      the (single) LiftedProperty's IrFormula out.
  //
  //   4. propertyHashFromFormula(formula) for the propertyHash.
  //      bindingHashFromAst(<the function declaration>) for the
  //      bindingHash.
  //
  //   5. mintMemento({ bindingHash, propertyHash, ... }) for the
  //      property memento. mintBridge({ sourceSymbol: shape.name, ... })
  //      for the bridge memento.
  //
  //   6. buildProofEnvelope([propertyMemento, bridgeMemento]) -> bytes.
  //      Write to outPath. Return CID.
  //
  // Until then, fail loudly so callers know the stub is not silent.
  throw new Error(
    "mint: not yet implemented. v0 ships Detect only; Mint lands run-2.",
  );
}
