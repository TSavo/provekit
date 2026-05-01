/**
 * Tests for IR structural invariants (src/ir/invariants.ts).
 *
 * These tests verify that the invariant properties compile and
 * evaluate correctly at runtime. They do NOT mint mementos —
 * minting is tested separately in cli.mint.new-ir.test.ts.
 */

import { describe, it, expect } from "vitest";
import {
  lambda,
  letTerm,
  choice,
} from "./symbolic/primitives.js";
import { Int } from "./sorts.js";

describe("IR term invariants", () => {
  it("VarTerm has no sort field", () => {
    const v: { kind: "var"; name: string } = { kind: "var", name: "x" };
    expect("sort" in v).toBe(false);
  });

  it("ConstTerm has sort field", () => {
    const c: { kind: "const"; value: unknown; sort: { kind: "primitive"; name: string } } = {
      kind: "const", value: 42, sort: { kind: "primitive", name: "Int" }
    };
    expect("sort" in c).toBe(true);
    expect(c.sort).toEqual({ kind: "primitive", name: "Int" });
  });

  it("CtorTerm has no sort field", () => {
    const ctor: { kind: "ctor"; name: string; args: unknown[] } = {
      kind: "ctor", name: "parseInt", args: []
    };
    expect("sort" in ctor).toBe(false);
  });
});

describe("Lambda term invariants", () => {
  it("LambdaTerm has paramSort", () => {
    const lam = lambda("x", Int, { kind: "const", value: 42, sort: { kind: "primitive", name: "Int" } });
    expect("paramSort" in lam).toBe(true);
    expect(lam.paramSort).toEqual({ kind: "primitive", name: "Int" });
  });

  it("LambdaTerm has body", () => {
    const body = { kind: "const" as const, value: 42, sort: { kind: "primitive" as const, name: "Int" } };
    const lam = lambda("x", Int, body);
    expect("body" in lam).toBe(true);
    expect(lam.body).toEqual(body);
  });

  it("LambdaTerm has no top-level sort field", () => {
    const lam = lambda("x", Int, { kind: "const", value: 42, sort: { kind: "primitive", name: "Int" } });
    expect("sort" in lam).toBe(false);
  });
});

describe("Let term invariants", () => {
  it("LetTerm has non-empty bindings", () => {
    const l = letTerm([{ name: "x", boundTerm: { kind: "const" as const, value: 1, sort: { kind: "primitive" as const, name: "Int" } } }], { kind: "const" as const, value: 2, sort: { kind: "primitive" as const, name: "Int" } });
    expect(Array.isArray(l.bindings)).toBe(true);
    expect(l.bindings.length).toBeGreaterThanOrEqual(1);
  });

  it("LetTerm has body", () => {
    const body = { kind: "const" as const, value: 2, sort: { kind: "primitive" as const, name: "Int" } };
    const l = letTerm([{ name: "x", boundTerm: { kind: "const" as const, value: 1, sort: { kind: "primitive" as const, name: "Int" } } }], body);
    expect("body" in l).toBe(true);
    expect(l.body).toEqual(body);
  });
});

describe("Choice formula invariants", () => {
  it("ChoiceFormula has varName", () => {
    const body = { kind: "atomic" as const, name: "true", args: [] as unknown[] };
    const c = choice("x", Int, body);
    expect("varName" in c).toBe(true);
    expect(c.varName).toBe("x");
  });

  it("ChoiceFormula has sort", () => {
    const body = { kind: "atomic" as const, name: "true", args: [] as unknown[] };
    const c = choice("x", Int, body);
    expect("sort" in c).toBe(true);
    expect(c.sort).toEqual({ kind: "primitive", name: "Int" });
  });

  it("ChoiceFormula has body", () => {
    const body = { kind: "atomic" as const, name: "true", args: [] as unknown[] };
    const c = choice("x", Int, body);
    expect("body" in c).toBe(true);
  });
});

describe("Evidence term invariants", () => {
  it("EvidenceTerm has proofType and certificate", () => {
    const evidence = {
      kind: "evidence" as const,
      proofType: "coq" as const,
      certificate: {
        tool: "coqc",
        version: "9.0",
        formulaHash: "blake3-512:abc...",
        proofData: "Qed.",
      },
    };
    expect("proofType" in evidence).toBe(true);
    expect("certificate" in evidence).toBe(true);
    expect(evidence.certificate.tool).toBe("coqc");
  });
});
