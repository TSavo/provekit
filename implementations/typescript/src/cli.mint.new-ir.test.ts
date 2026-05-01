// SPDX-License-Identifier: Apache-2.0
//
// cli.mint.new-ir.test.ts — Mint witnesses for new IR constructs
//
// Exercises the `provekit mint` CLI with lambda, let, and choice
// formulas to produce signed mementos (witnesses).

import { describe, it, expect } from "vitest";
import { mintMemento } from "./claimEnvelope/index.js";
import { computeCid } from "./canonicalizer/hash.js";
import { generateKeypair } from "./producerKeys/index.js";
import type { ClaimEnvelope } from "./claimEnvelope/types.js";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeKeypair() {
  return generateKeypair();
}

function envelopeCid(env: ClaimEnvelope): string {
  return env.cid;
}

// ---------------------------------------------------------------------------
// Lambda term witness
// ---------------------------------------------------------------------------

describe("mint lambda witness", () => {
  it("mints a signed memento for a lambda term property", () => {
    const { privateKey } = makeKeypair();

    // Property: lambda(x: Int, 42) is well-formed
    const lambdaFormula = {
      kind: "lambda",
      paramName: "x",
      paramSort: { kind: "primitive", name: "Int" },
      body: { kind: "const", value: 42, sort: { kind: "primitive", name: "Int" } },
    };

    const propertyHash = computeCid(
      Buffer.from(JSON.stringify(lambdaFormula), "utf8"),
    );

    const memento = mintMemento({
      bindingHash: "lambda-wellformed",
      propertyHash,
      verdict: "holds",
      producedBy: "provekit-ir-test@1.0",
      inputCids: [],
      evidence: {
        kind: "test-pass",
        schema: "blake3-512:test-pass-schema-v1",
        body: {
          runner: "vitest",
          runnerVersion: "1.0",
          testId: "lambda-structural",
          durationMs: 1,
        },
      },
      privateKey,
    });

    expect(memento.schemaVersion).toBe("1");
    expect(memento.verdict).toBe("holds");
    expect(memento.producerSignature).toBeTruthy();
    expect(memento.cid).toMatch(/^blake3-512:[a-f0-9]{128}$/);
  });
});

// ---------------------------------------------------------------------------
// Let term witness
// ---------------------------------------------------------------------------

describe("mint let witness", () => {
  it("mints a signed memento for a let term property", () => {
    const { privateKey } = makeKeypair();

    // Property: let x = 1 in x is well-formed
    const letFormula = {
      kind: "let",
      bindings: [
        { name: "x", boundTerm: { kind: "const", value: 1, sort: { kind: "primitive", name: "Int" } } },
      ],
      body: { kind: "var", name: "x" },
    };

    const propertyHash = computeCid(
      Buffer.from(JSON.stringify(letFormula), "utf8"),
    );

    const memento = mintMemento({
      bindingHash: "let-wellformed",
      propertyHash,
      verdict: "holds",
      producedBy: "provekit-ir-test@1.0",
      inputCids: [],
      evidence: {
        kind: "test-pass",
        schema: "blake3-512:test-pass-schema-v1",
        body: {
          runner: "vitest",
          runnerVersion: "1.0",
          testId: "let-structural",
          durationMs: 1,
        },
      },
      privateKey,
    });

    expect(memento.cid).toBeTruthy();
    expect(envelopeCid(memento)).toBe(memento.cid);
  });
});

// ---------------------------------------------------------------------------
// Choice formula witness
// ---------------------------------------------------------------------------

describe("mint choice witness", () => {
  it("mints a signed memento for a choice formula property", () => {
    const { privateKey } = makeKeypair();

    // Property: εx:Int. x > 0 is well-formed
    const choiceFormula = {
      kind: "choice",
      varName: "x",
      sort: { kind: "primitive", name: "Int" },
      body: {
        kind: "atomic",
        name: ">",
        args: [
          { kind: "var", name: "x" },
          { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
        ],
      },
    };

    const propertyHash = computeCid(
      Buffer.from(JSON.stringify(choiceFormula), "utf8"),
    );

    const memento = mintMemento({
      bindingHash: "choice-wellformed",
      propertyHash,
      verdict: "holds",
      producedBy: "provekit-ir-test@1.0",
      inputCids: [],
      evidence: {
        kind: "test-pass",
        schema: "blake3-512:test-pass-schema-v1",
        body: {
          runner: "vitest",
          runnerVersion: "1.0",
          testId: "choice-structural",
          durationMs: 1,
        },
      },
      privateKey,
    });

    expect(memento.evidence.kind).toBe("test-pass");
    expect(memento.cid).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// Evidence term witness (with formula hash matching)
// ---------------------------------------------------------------------------

describe("mint evidence witness", () => {
  it("mints a witness with evidence that references formula by hash", () => {
    const { privateKey } = makeKeypair();

    const formula = {
      kind: "atomic",
      name: "roundTrips",
      args: [{ kind: "var", name: "s" }],
    };

    const formulaHash = computeCid(
      Buffer.from(JSON.stringify(formula), "utf8"),
    );

    // Evidence that carries the formula hash
    const memento = mintMemento({
      bindingHash: "evidence-hash-match",
      propertyHash: formulaHash,
      verdict: "holds",
      producedBy: "provekit-ir-test@1.0",
      inputCids: [],
      evidence: {
        kind: "implication",
        schema: "blake3-512:implication-schema-v1",
        body: {
          antecedentHash: formulaHash,
          consequentHash: formulaHash,
          antecedentCid: "blake3-512:abc123...",
          consequentCid: "blake3-512:def456...",
          antecedentSlot: "pre",
          consequentSlot: "post",
          prover: "coq",
          proverRunMs: 100,
          proofWitness: "Qed.",
        },
      },
      privateKey,
    });

    expect(memento.propertyHash).toBe(formulaHash);
    expect(memento.evidence.kind).toBe("implication");
    const imp = memento.evidence as { body: { antecedentHash: string } };
    expect(imp.body.antecedentHash).toBe(formulaHash);
  });
});

// ---------------------------------------------------------------------------
// Contract evidence with lambda/let/choice
// ---------------------------------------------------------------------------

describe("mint contract witness with new IR", () => {
  it("mints a contract memento containing a lambda precondition", () => {
    const { privateKey } = makeKeypair();

    const pre = {
      kind: "forall",
      name: "f",
      sort: { kind: "primitive", name: "Int" },
      body: {
        kind: "atomic",
        name: ">",
        args: [
          {
            kind: "let",
            bindings: [
              {
                name: "result",
                boundTerm: {
                  kind: "lambda",
                  paramName: "x",
                  paramSort: { kind: "primitive", name: "Int" },
                  body: { kind: "var", name: "x" },
                },
              },
            ],
            body: { kind: "var", name: "result" },
          },
          { kind: "const", value: 0, sort: { kind: "primitive", name: "Int" } },
        ],
      },
    };

    const propertyHash = computeCid(Buffer.from(JSON.stringify(pre), "utf8"));

    const memento = mintMemento({
      bindingHash: "contract-lambda-let",
      propertyHash,
      verdict: "holds",
      producedBy: "provekit-ir-test@1.0",
      inputCids: [],
      evidence: {
        kind: "contract",
        schema: "blake3-512:contract-schema-v1",
        body: {
          contractName: "lambda_identity",
          outBinding: "out",
          pre,
          authoring: {
            producerKind: "kit-author",
            author: "provekit-test",
          },
        },
      },
      privateKey,
    });

    expect(memento.evidence.kind).toBe("contract");
    const contract = memento.evidence as { body: { pre: unknown } };
    expect(contract.body.pre).toBeDefined();
  });
});
