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
  liftTypeScriptLibraryBindingsText,
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
  it("lifts TypeScript sugar bindings into library-sugar-binding entries from real source", () => {
    const source = `
import Database from "better-sqlite3";
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function selectRows(db: Database.Database, sql: string, args: unknown[]) {
  return db.prepare(sql).all(args);
}
`;

    const result = liftTypeScriptLibraryBindingsText(source, "src/sqlite.ts");

    expect(result.refusals).toEqual([]);
    expect(result.declarations).toEqual([]);
    expect(result.libraryBindings).toHaveLength(1);
    const binding = result.libraryBindings[0]!;
    expect(binding.kind).toBe("library-sugar-binding-entry");
    expect(binding.target_language).toBe("typescript");
    expect(binding.target_library_tag).toBe("better-sqlite3");
    expect(binding.concept_name).toBe("concept:sql-query");
    expect(binding.source_function_name).toBe("selectRows");
    expect(binding.param_names).toEqual(["db", "sql", "args"]);
    expect(binding.param_types).toEqual(["Database.Database", "string", "unknown[]"]);
    expect(binding.return_type).toBe("unknown");
    expect(binding.term_shape_cid).toBe(canonicalCid(binding.term_shape));
    expect(binding.signature_shape_cid).toBe(canonicalCid(binding.signature_shape));
    expect(binding.body_source.file).toBe("src/sqlite.ts");
    expect(binding.body_source.locator).toEqual({ start_line: 5, start_col: 0, end_line: 8, end_col: 1 });
    const expectedSpan = source.split(/(?<=\n)/).slice(4, 8).join("").replace(/\n$/, "");
    expect(binding.body_source.source_cid).toBe(computeCid(Buffer.from(expectedSpan, "utf8")));
    expect(canonicalJsonString(binding)).not.toContain("emission_template");
  });

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

  it("round-trips a multi-statement body through the AST printer path (compileTypeScriptSourceIr without source-unit bytes)", () => {
    // The tautology this replaces: compileTypeScriptSourceIr short-circuits through the
    // stored bytes when a ts:source-unit is present, so it never exercised the AST printer.
    // This test drives the real compile path by stripping the source-unit memento so
    // compileFunctionContract + emitStatementsFromTerm must reconstruct TypeScript from the IR.
    const source = `function compute(x: number, y: number): number {
  let a = x + y;
  if (a > 0) {
    a = a - 1;
  } else {
    a = a + 1;
  }
  return a;
}
`;
    const first = liftTypeScriptSourceText(source, "src/roundtrip.ts");
    expect(first.refusals).toEqual([]);

    // Strip the source-unit memento: forces compileTypeScriptSourceIr onto the AST printer path.
    const contractsOnly = first.declarations.filter((d) => !d.fnName.endsWith(":<source-unit>"));
    expect(contractsOnly).toHaveLength(1);

    const compiled = compileTypeScriptSourceIr(contractsOnly);
    const second = liftTypeScriptSourceText(compiled, "src/roundtrip.ts");

    expect(second.refusals).toEqual([]);
    const secondContractsOnly = second.declarations.filter((d) => !d.fnName.endsWith(":<source-unit>"));
    expect(secondContractsOnly).toHaveLength(1);

    // The body term (rhs of the postcondition) must be structurally identical.
    expect(canonicalJsonString(rhs(secondContractsOnly[0]!))).toBe(
      canonicalJsonString(rhs(contractsOnly[0]!)),
    );
    // And its content-address must match.
    expect(canonicalCid(rhs(secondContractsOnly[0]!))).toBe(
      canonicalCid(rhs(contractsOnly[0]!)),
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
