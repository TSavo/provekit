import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { canonicalEncode, canonicalJsonString } from "../../claimEnvelope/canonicalize.js";
import { computeCid } from "../../canonicalizer/hash.js";
import {
  compileTypeScriptSourceBodyIr,
  compileTypeScriptSourceIr,
  functionContractCid,
  liftTypeScriptSourcePaths,
  liftTypeScriptSourceText,
} from "./index.js";

function tempDir(prefix = "provekit-ts-source-"): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

function rhs(contract: Record<string, any>): any {
  return contract.post.args[1];
}

function canonicalCid(value: unknown): string {
  return computeCid(canonicalEncode(value));
}

describe("typescript-source lifter", () => {
  it("lifts function declarations into ts:-namespaced function-contracts wrapped by source-unit", () => {
    const result = liftTypeScriptSourceText(
      "export function add(x: number, y: number): number { return x + y; }\n",
      "src/math.ts",
    );

    expect(result.refusals).toEqual([]);
    expect(result.declarations).toHaveLength(2);

    const sourceUnit = result.declarations[0]!;
    expect(sourceUnit.kind).toBe("function-contract");
    expect(sourceUnit.fnName).toBe("src/math.ts:<source-unit>");
    expect(rhs(sourceUnit).name).toBe("ts:source-unit");
    expect(rhs(sourceUnit).args[1].name).toBe("ts:seq");

    const add = result.declarations[1]!;
    expect(add.fnName).toBe("src/math.ts:add");
    expect(add.formals).toEqual(["x", "y"]);
    expect(add.formalSorts).toEqual([
      { kind: "primitive", name: "Number" },
      { kind: "primitive", name: "Number" },
    ]);
    expect(add.returnSort).toEqual({ kind: "primitive", name: "Number" });
    expect(rhs(add)).toMatchObject({ kind: "ctor", name: "ts:add" });
    expect(functionContractCid(add)).toMatch(/^blake3-512:[0-9a-f]{128}$/);
  });

  it("qualifies class and namespace methods so distinct definitions do not collide", () => {
    const result = liftTypeScriptSourceText(
      `
      namespace N {
        export class A { m(x: number): number { return x + 1; } }
        export class B { m(x: number): number { return x + 2; } }
      }
      `,
      "src/classes.ts",
    );

    expect(result.refusals).toEqual([]);
    expect(result.declarations.map((d) => d.fnName)).toEqual([
      "src/classes.ts:<source-unit>",
      "src/classes.ts:N.A.m",
      "src/classes.ts:N.B.m",
    ]);
  });

  it("emits canonical Effect wire shapes sorted like the Rust Effect::sort_key", () => {
    const result = liftTypeScriptSourceText(
      `
      let counter = 0;
      function tick(x: number): number {
        counter = counter + x;
        console.log(counter);
        while (x > 0) {
          x = x - 1;
        }
        missing(counter);
        if (x < 0) { throw "negative"; }
        return counter;
      }
      `,
      "src/effects.ts",
    );

    expect(result.refusals).toEqual([]);
    const tick = result.declarations.find((d) => d.fnName === "src/effects.ts:tick")!;
    expect(tick.effects).toEqual([
      { kind: "reads", target: "src/effects.ts:counter" },
      { kind: "writes", target: "src/effects.ts:counter" },
      { kind: "io" },
      { kind: "panics" },
      { kind: "unresolved_call", name: "missing" },
      { kind: "opaque_loop", loopCid: expect.stringMatching(/^blake3-512:[0-9a-f]{128}$/) },
    ]);
  });

  it("refuses unsupported syntax instead of emitting unknown or skip fallbacks", () => {
    const result = liftTypeScriptSourceText(
      "const f = (x: number): number => x + 1;\n",
      "src/refuse.ts",
    );

    expect(result.declarations).toEqual([]);
    expect(result.refusals).toEqual([
      {
        kind: "ArrowFunction",
        function: "f",
        line: 1,
        reason: expect.stringContaining("arrow functions"),
      },
    ]);
    expect(canonicalJsonString(result)).not.toContain("ts:unknown");
    expect(canonicalJsonString(result)).not.toContain("ts:skip");
  });

  it("emits distinct refusal reasons for unsupported function shapes", () => {
    const result = liftTypeScriptSourceText(
      `
      declare function dec(...args: any[]): any;
      async function asyncFn(x: number): Promise<number> { return x; }
      function* generatorFn(x: number): Iterable<number> { yield x; }
      function genericFn<T>(x: number): number { return x; }
      function restFn(...xs: number[]): number { return 1; }
      function defaultFn(x: number = 1): number { return x; }
      function destructuredFn({ x }: { x: number }): number { return x; }
      class C {
        @dec
        decoratedMethod(x: number): number { return x; }
      }
      `,
      "src/refuse-shapes.ts",
    );

    expect(result.declarations).toEqual([]);
    expect(new Map(result.refusals.map((refusal) => [refusal.function, refusal.reason]))).toEqual(new Map([
      ["src/refuse-shapes.ts:asyncFn", "async function not supported"],
      ["src/refuse-shapes.ts:generatorFn", "generator function not supported"],
      ["src/refuse-shapes.ts:genericFn", "generic type parameters not supported"],
      ["src/refuse-shapes.ts:restFn", "rest parameters not supported"],
      ["src/refuse-shapes.ts:defaultFn", "default parameters not supported"],
      ["src/refuse-shapes.ts:destructuredFn", "destructured parameters not supported"],
      ["src/refuse-shapes.ts:C.decoratedMethod", "decorated function not supported"],
    ]));
  });

  it("preserves source-unit raw bytes for lift(compile(lift(src))) round-trips", () => {
    const source = "function add(x: number, y: number): number { return x + y; }\n";
    const first = liftTypeScriptSourceText(source, "src/roundtrip.ts");
    expect(first.refusals).toEqual([]);

    const compiled = compileTypeScriptSourceIr(first.declarations);
    const second = liftTypeScriptSourceText(compiled, "src/roundtrip.ts");

    expect(compiled).toBe(source);
    expect(second.refusals).toEqual([]);
    expect(canonicalJsonString(second.declarations)).toBe(
      canonicalJsonString(first.declarations),
    );
    expect(second.declarations.map(functionContractCid)).toEqual(
      first.declarations.map(functionContractCid),
    );
  });

  it("round-trips a bare body term through the AST printer with byte-identical canonical IR", () => {
    const first = liftTypeScriptSourceText(
      "function f(x: number, y: number): number { return x + y; }\n",
      "src/body-roundtrip.ts",
    );
    expect(first.refusals).toEqual([]);

    const firstContract = first.declarations.find((decl) => decl.fnName === "src/body-roundtrip.ts:f")!;
    const originalBodyTerm = rhs(firstContract);
    const compiled = compileTypeScriptSourceBodyIr(originalBodyTerm, {
      functionName: "f",
      formals: firstContract.formals,
      formalSorts: firstContract.formalSorts,
      returnSort: firstContract.returnSort,
    });
    const second = liftTypeScriptSourceText(compiled, "src/body-roundtrip.ts");

    expect(second.refusals).toEqual([]);
    const secondContract = second.declarations.find((decl) => decl.fnName === "src/body-roundtrip.ts:f")!;
    const reliftedBodyTerm = rhs(secondContract);
    expect([...canonicalEncode(reliftedBodyTerm)]).toEqual([...canonicalEncode(originalBodyTerm)]);
    expect(canonicalCid(reliftedBodyTerm)).toBe(canonicalCid(originalBodyTerm));
  });

  it("rejects source paths that escape the workspace root", () => {
    const td = tempDir();
    writeFileSync(join(td, "ok.ts"), "function ok(): number { return 1; }\n");

    const result = liftTypeScriptSourcePaths(td, ["../escape.ts"]);

    expect(result.declarations).toEqual([]);
    expect(result.refusals[0]).toMatchObject({
      kind: "path-traversal",
      function: null,
      line: null,
    });
  });
});
