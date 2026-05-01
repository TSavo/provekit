/**
 * MementoPool — the verification state machine.
 *
 * Architecture principle: the memento IS the verification.
 * To verify something is to find its memento in the pool.
 * The .proof protocol IS the cache.
 * The hash IS the boundary.
 *
 * The pool indexes mementos by:
 *   - memento CID (the envelope's own content hash)
 *   - formula CID (the hash of the formula that the memento verifies)
 *
 * Systems don't exchange formulas; they exchange hashes.
 * Trust is optional at every boundary.
 */

import { computeCid } from "../canonicalizer/hash.js";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import type { ClaimEnvelope } from "../claimEnvelope/types.js";

export interface MementoPool {
  /** CID → memento envelope. The memento IS the verification. */
  mementos: Record<string, ClaimEnvelope>;
  /** Formula CID → memento CID. Index for fast formula lookup. */
  formulaToMemento: Record<string, string>;
  /** sourceSymbol → bridge envelope. */
  bridgesBySymbol: Record<string, ClaimEnvelope>;
  errors: Array<{ proofFile: string; reason: string }>;
}

export function createMementoPool(): MementoPool {
  return {
    mementos: {},
    formulaToMemento: {},
    bridgesBySymbol: {},
    errors: [],
  };
}

/** Compute the CID for a formula (any JSON value). The hash IS the boundary. */
export function computeFormulaCid(formula: unknown): string {
  const bytes = canonicalEncode(formula);
  return computeCid(bytes);
}

/** Insert a memento into the pool and index it by formula hash. */
export function insertMemento(pool: MementoPool, mementoCid: string, envelope: ClaimEnvelope): void {
  // Index by the formula hashes referenced in the evidence body
  const evidence = envelope.evidence;
  if (evidence?.kind === "contract" || evidence?.kind === "implication") {
    const body = evidence.body as Record<string, unknown>;
    const hashFields = [
      "preHash", "postHash", "invHash",
      "antecedentHash", "consequentHash",
    ];
    for (const field of hashFields) {
      const hash = body[field];
      if (typeof hash === "string") {
        pool.formulaToMemento[hash] = mementoCid;
      }
    }
  }
  pool.mementos[mementoCid] = envelope;
}

/**
 * The fundamental verification operation: look up a formula by its
 * content hash. The memento IS the verification; if found, the
 * formula is verified. No solver is invoked.
 */
export function verifyByHash(pool: MementoPool, formulaCid: string): ClaimEnvelope | undefined {
  const mementoCid = pool.formulaToMemento[formulaCid];
  if (!mementoCid) return undefined;
  return pool.mementos[mementoCid];
}

/**
 * Compute CID for a formula, then look it up.
 * The canonicalization + hash IS the boundary between systems.
 */
export function verifyFormula(pool: MementoPool, formula: unknown): ClaimEnvelope | undefined {
  const cid = computeFormulaCid(formula);
  return verifyByHash(pool, cid);
}

/**
 * Sub-formula composition: walk the formula DAG and return all
 * sub-formula CIDs that have mementos in the pool. If P is verified
 * and we need to prove P ∧ Q, this returns P's CID so the solver
 * can focus on Q.
 */
export function findVerifiedSubformulas(
  pool: MementoPool,
  formula: unknown,
): Array<{ cid: string; memento: ClaimEnvelope }> {
  const verified: Array<{ cid: string; memento: ClaimEnvelope }> = [];
  const stack: unknown[] = [formula];
  const visited = new Set<string>();

  while (stack.length > 0) {
    const node = stack.pop()!;
    const cid = computeFormulaCid(node);
    if (visited.has(cid)) continue;
    visited.add(cid);

    const memento = verifyByHash(pool, cid);
    if (memento) {
      verified.push({ cid, memento });
    }

    // Push children for recursive checking
    if (node && typeof node === "object" && !Array.isArray(node)) {
      const obj = node as Record<string, unknown>;
      const kind = obj.kind;
      if (kind === "and" || kind === "or" || kind === "not" || kind === "implies") {
        const operands = obj.operands;
        if (Array.isArray(operands)) {
          for (const op of operands) stack.push(op);
        }
      } else if (kind === "forall" || kind === "exists" || kind === "choice") {
        if (obj.body) stack.push(obj.body);
      }
    }
  }

  return verified;
}
