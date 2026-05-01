/**
 * Validation rules for claim envelopes.
 *
 * Spec: protocol/specs/2026-04-29-universal-claim-envelope.md §Validation rules
 *
 * Validates:
 * 1. Wrapper shape (required fields, types, verdict enum, producedBy format).
 * 2. CID integrity (recompute and compare).
 * 3. Signature (when present, via optional KeyResolver).
 * 4. Variant schema (standard variants checked against built-in schemas).
 * 5. inputCids consistency (each is 32 hex chars; deeper DAG validation optional).
 */

import { KeyObject } from "node:crypto";
import { VERDICTS, type ClaimEnvelope, type EvidenceVariant } from "./types.js";
import { computeEnvelopeCid } from "./cid.js";
import { verifyEnvelopeSignature } from "./sign.js";

// ---------------------------------------------------------------------------
// KeyResolver callback
// ---------------------------------------------------------------------------

/**
 * Called during validation when a `producerSignature` is present.
 * Return the public key for the given `producedBy` identity, or
 * null if the key is unknown (signature check is skipped / warned).
 */
export type KeyResolver = (
  producedBy: string,
) => KeyObject | Buffer | string | null;

// ---------------------------------------------------------------------------
// Validation result
// ---------------------------------------------------------------------------

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Protocol v1.1.0 self-identifying hash:
 *   <algorithm>-<bits>:<lowercase-hex-digest>
 * v1.1.0 ships with `blake3-512` (full 64-byte / 128 hex BLAKE3 digest)
 * as the only permitted tag. Every hash field in the protocol uses this
 * one regex.
 */
const SELF_IDENTIFYING_HASH = /^[a-z0-9]+-[0-9]+:[0-9a-f]+$/;
/**
 * Protocol v1.1.0 self-identifying signature/pubkey:
 *   <algorithm>:<base64-payload>
 * v1.1.0 ships with `ed25519` as the only permitted tag.
 */
const SELF_IDENTIFYING_SIG = /^[a-z0-9]+:[A-Za-z0-9+/]+=*$/;
/** producedBy format: <name>@<version>; name may include colons, slashes, dots. */
const PRODUCED_BY = /^[^@\s]+@[^@\s]+$/;
/** ISO-8601 UTC basic check */
const ISO8601 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/;

function isString(v: unknown): v is string {
  return typeof v === "string";
}

function isNumber(v: unknown): v is number {
  return typeof v === "number";
}

function isBoolean(v: unknown): v is boolean {
  return typeof v === "boolean";
}

function isObject(v: unknown): v is Record<string, unknown> {
  return v !== null && typeof v === "object" && !Array.isArray(v);
}

function isStringArray(v: unknown): v is string[] {
  return Array.isArray(v) && v.every((x) => typeof x === "string");
}

// ---------------------------------------------------------------------------
// Variant body validators
// ---------------------------------------------------------------------------

function validateVariantBody(
  evidence: EvidenceVariant,
  errors: string[],
): void {
  const { kind, body } = evidence as { kind: string; body: Record<string, unknown> };

  if (!isObject(body)) {
    errors.push(`evidence.body must be an object for kind "${kind}"`);
    return;
  }

  switch (kind) {
    case "z3-model": {
      if (!isString(body.smtLibInput)) errors.push("z3-model body.smtLibInput must be string");
      if (body.z3Verdict !== "sat") errors.push("z3-model body.z3Verdict must be \"sat\"");
      if (!isString(body.model)) errors.push("z3-model body.model must be string");
      if (!isObject(body.counterexample)) errors.push("z3-model body.counterexample must be object");
      if (!isNumber(body.z3RunMs)) errors.push("z3-model body.z3RunMs must be number");
      break;
    }
    case "z3-unsat": {
      if (!isString(body.smtLibInput)) errors.push("z3-unsat body.smtLibInput must be string");
      if (body.z3Verdict !== "unsat") errors.push("z3-unsat body.z3Verdict must be \"unsat\"");
      if (body.proof !== undefined && !isString(body.proof)) errors.push("z3-unsat body.proof must be string when present");
      if (!isNumber(body.z3RunMs)) errors.push("z3-unsat body.z3RunMs must be number");
      break;
    }
    case "pattern-match": {
      if (!isString(body.pattern)) errors.push("pattern-match body.pattern must be string");
      if (!isStringArray(body.matchedNodes)) errors.push("pattern-match body.matchedNodes must be string[]");
      if (!isObject(body.matchedCaptures)) errors.push("pattern-match body.matchedCaptures must be object");
      break;
    }
    case "type-check-pass": {
      if (!isString(body.checker)) errors.push("type-check-pass body.checker must be string");
      if (!isString(body.checkerVersion)) errors.push("type-check-pass body.checkerVersion must be string");
      if (!isString(body.symbol)) errors.push("type-check-pass body.symbol must be string");
      if (body.resolvedType !== undefined && !isString(body.resolvedType)) errors.push("type-check-pass body.resolvedType must be string when present");
      if (body.diagnosticsClean !== true) errors.push("type-check-pass body.diagnosticsClean must be true");
      break;
    }
    case "lint-pass": {
      if (!isString(body.linter)) errors.push("lint-pass body.linter must be string");
      if (!isString(body.linterVersion)) errors.push("lint-pass body.linterVersion must be string");
      if (!isString(body.rulesetHash) || !SELF_IDENTIFYING_HASH.test(body.rulesetHash as string)) errors.push("lint-pass body.rulesetHash must be a self-identifying hash");
      if (body.warnings !== 0) errors.push("lint-pass body.warnings must be 0");
      break;
    }
    case "test-pass": {
      if (!isString(body.runner)) errors.push("test-pass body.runner must be string");
      if (!isString(body.runnerVersion)) errors.push("test-pass body.runnerVersion must be string");
      if (!isString(body.testId)) errors.push("test-pass body.testId must be string");
      if (!isNumber(body.durationMs)) errors.push("test-pass body.durationMs must be number");
      if (body.stdout !== undefined && !isString(body.stdout)) errors.push("test-pass body.stdout must be string when present");
      break;
    }
    case "test-fail": {
      if (!isString(body.runner)) errors.push("test-fail body.runner must be string");
      if (!isString(body.runnerVersion)) errors.push("test-fail body.runnerVersion must be string");
      if (!isString(body.testId)) errors.push("test-fail body.testId must be string");
      if (!isNumber(body.durationMs)) errors.push("test-fail body.durationMs must be number");
      if (body.stdout !== undefined && !isString(body.stdout)) errors.push("test-fail body.stdout must be string when present");
      if (body.failureDetail !== undefined && !isString(body.failureDetail)) errors.push("test-fail body.failureDetail must be string when present");
      break;
    }
    case "llm-proposal": {
      if (!isString(body.llm)) errors.push("llm-proposal body.llm must be string");
      if (!isString(body.llmVersion)) errors.push("llm-proposal body.llmVersion must be string");
      if (!isString(body.promptCid) || !SELF_IDENTIFYING_HASH.test(body.promptCid as string)) errors.push("llm-proposal body.promptCid must be a self-identifying hash");
      if (!isString(body.proposedIrFormula)) errors.push("llm-proposal body.proposedIrFormula must be string");
      if (!isNumber(body.confidence) || (body.confidence as number) < 0 || (body.confidence as number) > 1) errors.push("llm-proposal body.confidence must be number 0..1");
      if (body.rationale !== undefined && !isString(body.rationale)) errors.push("llm-proposal body.rationale must be string when present");
      break;
    }
    case "mutation-witness": {
      if (!isString(body.testCid) || !SELF_IDENTIFYING_HASH.test(body.testCid as string)) errors.push("mutation-witness body.testCid must be a self-identifying hash");
      if (!isString(body.mutationCid) || !SELF_IDENTIFYING_HASH.test(body.mutationCid as string)) errors.push("mutation-witness body.mutationCid must be a self-identifying hash");
      if (!isBoolean(body.failsOnOriginal)) errors.push("mutation-witness body.failsOnOriginal must be boolean");
      if (!isBoolean(body.passesOnFixed)) errors.push("mutation-witness body.passesOnFixed must be boolean");
      break;
    }
    case "workflow-run": {
      if (!isString(body.workflowName)) errors.push("workflow-run body.workflowName must be string");
      if (!isString(body.workflowCid) || !SELF_IDENTIFYING_HASH.test(body.workflowCid as string)) errors.push("workflow-run body.workflowCid must be a self-identifying hash");
      if (!isObject(body.inputCanonicalForm)) errors.push("workflow-run body.inputCanonicalForm must be object");
      // body.output is type-specific, no constraint
      break;
    }
    case "contract": {
      if (!isString(body.contractName)) errors.push("contract body.contractName must be string");
      if (!isString(body.outBinding)) errors.push("contract body.outBinding must be string");
      const hasPre = body.pre !== undefined;
      const hasPost = body.post !== undefined;
      const hasInv = body.inv !== undefined;
      if (!hasPre && !hasPost && !hasInv) {
        errors.push("contract body must have at least one of pre/post/inv");
      }
      if (hasPre && (!isString(body.preHash) || !SELF_IDENTIFYING_HASH.test(body.preHash as string))) {
        errors.push("contract body.preHash must be a self-identifying hash when pre is present");
      }
      if (hasPost && (!isString(body.postHash) || !SELF_IDENTIFYING_HASH.test(body.postHash as string))) {
        errors.push("contract body.postHash must be a self-identifying hash when post is present");
      }
      if (hasInv && (!isString(body.invHash) || !SELF_IDENTIFYING_HASH.test(body.invHash as string))) {
        errors.push("contract body.invHash must be a self-identifying hash when inv is present");
      }
      if (!isObject(body.authoring)) {
        errors.push("contract body.authoring must be a tagged authoring block");
      } else {
        const a = body.authoring as Record<string, unknown>;
        const pk = a.producerKind;
        if (pk !== "kit-author" && pk !== "lift" && pk !== "llm") {
          errors.push(`contract body.authoring.producerKind must be one of "kit-author"|"lift"|"llm" (got ${JSON.stringify(pk)})`);
        }
      }
      break;
    }
    case "implication": {
      if (!isString(body.antecedentHash) || !SELF_IDENTIFYING_HASH.test(body.antecedentHash as string)) {
        errors.push("implication body.antecedentHash must be a self-identifying hash");
      }
      if (!isString(body.consequentHash) || !SELF_IDENTIFYING_HASH.test(body.consequentHash as string)) {
        errors.push("implication body.consequentHash must be a self-identifying hash");
      }
      if (!isString(body.antecedentCid) || !SELF_IDENTIFYING_HASH.test(body.antecedentCid as string)) {
        errors.push("implication body.antecedentCid must be a self-identifying hash");
      }
      if (!isString(body.consequentCid) || !SELF_IDENTIFYING_HASH.test(body.consequentCid as string)) {
        errors.push("implication body.consequentCid must be a self-identifying hash");
      }
      const slotOk = (s: unknown) => s === "pre" || s === "post" || s === "inv";
      if (!slotOk(body.antecedentSlot)) errors.push('implication body.antecedentSlot must be "pre"|"post"|"inv"');
      if (!slotOk(body.consequentSlot)) errors.push('implication body.consequentSlot must be "pre"|"post"|"inv"');
      if (!isString(body.prover)) errors.push("implication body.prover must be string");
      if (!isNumber(body.proverRunMs)) errors.push("implication body.proverRunMs must be number");
      break;
    }
    case "legacy-witness":
    case "property": {
      errors.push(
        `evidence.kind "${kind}" was removed in protocol v1.1; producers must emit "contract"`,
      );
      break;
    }
    default:
      // Unknown variant — verdict-trustworthy but witness-opaque.
      // No error; future variants are allowed.
      break;
  }
}

// ---------------------------------------------------------------------------
// Main validator
// ---------------------------------------------------------------------------

/**
 * Validate a claim envelope per the spec's §Validation rules.
 *
 * @param envelope - The envelope to validate (any shape, cast internally).
 * @param opts.keyResolver - Called to resolve a public key for signature
 *   verification. If absent, signatures are skipped with a warning.
 * @param opts.swarmMode - If true, missing or unresolvable signature is an
 *   error rather than a warning (required for swarm-distribution).
 */
export function validateEnvelope(
  envelope: unknown,
  opts: {
    keyResolver?: KeyResolver;
    swarmMode?: boolean;
  } = {},
): ValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!isObject(envelope)) {
    return { valid: false, errors: ["envelope must be a non-null object"], warnings };
  }

  const env = envelope as Record<string, unknown>;

  // ------------------------------------------------------------------
  // 1. Wrapper shape
  // ------------------------------------------------------------------

  if (env.schemaVersion !== "1") {
    errors.push(`schemaVersion must be "1", got: ${JSON.stringify(env.schemaVersion)}`);
  }

  if (!isString(env.bindingHash) || !SELF_IDENTIFYING_HASH.test(env.bindingHash)) {
    errors.push(`bindingHash must be a self-identifying hash (e.g. "blake3-512:..."), got: ${JSON.stringify(env.bindingHash)}`);
  }

  if (!isString(env.propertyHash) || !SELF_IDENTIFYING_HASH.test(env.propertyHash)) {
    errors.push(`propertyHash must be a self-identifying hash (e.g. "blake3-512:..."), got: ${JSON.stringify(env.propertyHash)}`);
  }

  if (!isString(env.verdict) || !VERDICTS.has(env.verdict as any)) {
    errors.push(
      `verdict must be one of [${[...VERDICTS].join(", ")}], got: ${JSON.stringify(env.verdict)}`,
    );
  }

  if (!isString(env.producedBy) || !PRODUCED_BY.test(env.producedBy)) {
    errors.push(
      `producedBy must match <name>@<version>, got: ${JSON.stringify(env.producedBy)}`,
    );
  }

  if (!isString(env.producedAt) || !ISO8601.test(env.producedAt)) {
    errors.push(`producedAt must be an ISO-8601 UTC string, got: ${JSON.stringify(env.producedAt)}`);
  }

  if (!Array.isArray(env.inputCids) || !env.inputCids.every((x: unknown) => isString(x) && SELF_IDENTIFYING_HASH.test(x))) {
    errors.push("inputCids must be an array of self-identifying hash strings");
  }

  if (!isObject(env.evidence)) {
    errors.push("evidence must be a non-null object");
  } else {
    const ev = env.evidence as Record<string, unknown>;
    if (!isString(ev.kind)) {
      errors.push("evidence.kind must be a string");
    }
    if (!isString(ev.schema) || !SELF_IDENTIFYING_HASH.test(ev.schema)) {
      errors.push("evidence.schema must be a self-identifying hash string");
    }
    if (ev.kind && !errors.some((e) => e.startsWith("evidence"))) {
      validateVariantBody(env.evidence as unknown as EvidenceVariant, errors);
    }
  }

  if (!isString(env.cid) || !SELF_IDENTIFYING_HASH.test(env.cid)) {
    errors.push(`cid must be a self-identifying hash (e.g. "blake3-512:..."), got: ${JSON.stringify(env.cid)}`);
  }

  if (env.producerSignature !== undefined) {
    if (!isString(env.producerSignature) || !SELF_IDENTIFYING_SIG.test(env.producerSignature)) {
      errors.push(`producerSignature must be a self-identifying signature (e.g. "ed25519:..."), got: ${JSON.stringify(env.producerSignature)}`);
    }
  }

  // ------------------------------------------------------------------
  // 2. CID integrity — only if wrapper shape is OK enough to canonicalize
  // ------------------------------------------------------------------

  const wrapperOk = errors.length === 0;
  if (wrapperOk) {
    const recomputed = computeEnvelopeCid(env as unknown as ClaimEnvelope);
    if (recomputed !== env.cid) {
      errors.push(
        `CID integrity failure: stored=${env.cid}, recomputed=${recomputed}`,
      );
    }
  }

  // ------------------------------------------------------------------
  // 3. Signature verification
  // ------------------------------------------------------------------

  if (isString(env.producerSignature)) {
    const resolver = opts.keyResolver;
    if (!resolver) {
      const msg = "producerSignature present but no KeyResolver provided; signature not verified";
      if (opts.swarmMode) errors.push(msg);
      else warnings.push(msg);
    } else if (isString(env.producedBy)) {
      const key = resolver(env.producedBy);
      if (!key) {
        const msg = `producer key not found for "${env.producedBy}"; signature not verified`;
        if (opts.swarmMode) errors.push(msg);
        else warnings.push(msg);
      } else {
        const ok = verifyEnvelopeSignature(env as unknown as ClaimEnvelope, key);
        if (!ok) {
          errors.push(`producerSignature verification failed for producer "${env.producedBy}"`);
        }
      }
    }
  } else if (opts.swarmMode) {
    warnings.push("no producerSignature present; swarm mode recommends signatures");
  }

  return { valid: errors.length === 0, errors, warnings };
}
