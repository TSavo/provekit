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
  liftTypeScriptLibraryBindingsPaths,
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
    expect(binding.term_shape).not.toBeNull();
    expect(binding.term_shape_cid).toBe(canonicalCid(binding.term_shape));
    expect(binding.signature_shape_cid).toBe(
      canonicalCid({ param_names: ["db", "sql", "args"], param_types: ["Database.Database", "string", "unknown[]"], return_type: "unknown" }),
    );
    expect(binding.loss_record_contribution).toEqual({ form: "literal", value: { entries: [] } });
    expect(binding.body_source.file).toBe("src/sqlite.ts");
    expect(binding.body_source.span).toEqual({ start_line: 5, start_col: 0, end_line: 8, end_col: 1 });
    const expectedSpan = source.split(/(?<=\n)/).slice(4, 8).join("").replace(/\n$/, "");
    expect(binding.body_source.source_cid).toBe(computeCid(Buffer.from(expectedSpan, "utf8")));
    expect(canonicalJsonString(binding)).not.toContain("emission_template");
  });

  // -----------------------------------------------------------------
  // #1357 / #1355: family + version axes on @sugar.bind decorators.
  // Parallel to walk_rpc's rust-side tests. Both fields are optional;
  // absent on decorator ↔ absent in emitted JSON.
  // -----------------------------------------------------------------

  it("lifts family + library_version when present on @sugar.bind", () => {
    const source = `
import Database from "better-sqlite3";
import { sugar } from "provekit";

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
function selectRows(db: Database.Database, sql: string, args: unknown[]) {
  return db.prepare(sql).all(args);
}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/sqlite.ts");
    expect(result.libraryBindings).toHaveLength(1);
    const binding = result.libraryBindings[0]! as Record<string, unknown>;
    expect(binding.family).toBe("concept:family:sql");
    expect(binding.library_version).toBe("12.9.0");
  });

  it("omits family + library_version when absent on @sugar.bind (back-compat)", () => {
    const source = `
import Database from "better-sqlite3";
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function selectRows(db: Database.Database, sql: string, args: unknown[]) {
  return db.prepare(sql).all(args);
}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/sqlite.ts");
    expect(result.libraryBindings).toHaveLength(1);
    const binding = result.libraryBindings[0]! as Record<string, unknown>;
    expect(binding.family).toBeUndefined();
    expect(binding.library_version).toBeUndefined();
  });

  it("does not emit library-sugar-binding entries for unannotated functions (discrimination)", () => {
    const source = `
function unannotated(db: unknown, sql: string): unknown[] {
  return [];
}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/no-sugar.ts");

    expect(result.libraryBindings).toHaveLength(0);
    expect(result.refusals).toHaveLength(0);
  });

  it("lifts library-sugar bindings from workspace paths without ordinary function contracts", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "shim.ts"),
      `
import Database from "better-sqlite3";
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-execute", library: "better-sqlite3" })
export function run(db: Database.Database, sql: string, args: unknown[]): unknown {
  return db.prepare(sql).run(args);
}
`,
      "utf8",
    );

    const result = liftTypeScriptLibraryBindingsPaths(td, ["."]);

    expect(result.declarations).toEqual([]);
    expect(result.libraryBindings).toHaveLength(1);
    expect(result.libraryBindings[0]).toMatchObject({
      kind: "library-sugar-binding-entry",
      target_language: "typescript",
      target_library_tag: "better-sqlite3",
      concept_name: "concept:sql-execute",
      source_function_name: "run",
      param_names: ["db", "sql", "args"],
    });
  });

  it("emits one entry per annotated function when multiple appear in the same file", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function queryAll(db: unknown, sql: string): unknown[] {
  return [];
}

function unannotated(): void {}

@sugar.bind({ concept: "concept:sql-execute", library: "better-sqlite3" })
function execute(db: unknown, sql: string): void {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/multi.ts");

    expect(result.libraryBindings).toHaveLength(2);
    expect(result.libraryBindings[0]!.source_function_name).toBe("queryAll");
    expect(result.libraryBindings[0]!.concept_name).toBe("concept:sql-query");
    expect(result.libraryBindings[1]!.source_function_name).toBe("execute");
    expect(result.libraryBindings[1]!.concept_name).toBe("concept:sql-execute");
  });

  it("ignores @sugar.bind decorators missing required concept or library fields", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query" })
function missingLibrary(x: string): string { return x; }

@sugar.bind({ library: "better-sqlite3" })
function missingConcept(x: string): string { return x; }

@sugar.bind({})
function missingBoth(x: string): string { return x; }
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/malformed.ts");

    expect(result.libraryBindings).toHaveLength(0);
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

  // loss / observed_dimension parsing tests

  it("emits empty loss entries when loss array is absent from @sugar.bind", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function queryAll(db: unknown, sql: string): unknown[] { return []; }
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/no-loss.ts");

    expect(result.libraryBindings).toHaveLength(1);
    const binding = result.libraryBindings[0]!;
    expect(binding.loss_record_contribution).toEqual({ form: "literal", value: { entries: [] } });
  });

  it("does not emit a binding for an unannotated function alongside an annotated one (discrimination)", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function annotated(db: unknown): unknown[] { return []; }

function bare(): void {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/mix.ts");

    expect(result.libraryBindings).toHaveLength(1);
    expect(result.libraryBindings[0]!.source_function_name).toBe("annotated");
  });

  it("emits multi-dim loss entries from the loss array on @sugar.bind", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-execute", library: "better-sqlite3", loss: ["async-shape", "error-kind"] })
function execute(db: unknown, sql: string): void {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/loss.ts");

    expect(result.libraryBindings).toHaveLength(1);
    const binding = result.libraryBindings[0]!;
    expect(binding.loss_record_contribution).toEqual({
      form: "literal",
      value: { entries: ["async-shape", "error-kind"] },
    });
  });

  it("does not treat a non-array loss field as entries (structural discrimination)", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-execute", library: "better-sqlite3", loss: "not-an-array" })
function execute(db: unknown, sql: string): void {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/bad-loss.ts");

    // loss field is ignored when it is not an array literal; entries must be empty
    expect(result.libraryBindings).toHaveLength(1);
    expect(result.libraryBindings[0]!.loss_record_contribution).toEqual({
      form: "literal",
      value: { entries: [] },
    });
  });

  it("emits observed_dimension on the binding entry when declared on @sugar.bind", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-changes-affected", library: "better-sqlite3", observed_dimension: "row-count" })
function changes(result: unknown): number { return 0; }
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/observed.ts");

    expect(result.libraryBindings).toHaveLength(1);
    const binding = result.libraryBindings[0]!;
    expect(binding.observed_dimension).toBe("row-count");
  });

  it("does not set observed_dimension when the field is absent from @sugar.bind (discrimination)", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function queryAll(db: unknown): unknown[] { return []; }
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/no-observed.ts");

    expect(result.libraryBindings).toHaveLength(1);
    expect(result.libraryBindings[0]!.observed_dimension).toBeUndefined();
  });

  // @sugar.refuse class decorator tests

  it("emits a refusal-memento from a @sugar.refuse class decorator", () => {
    const source = `
import { sugar } from "provekit";

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-physical-backup",
  reason: "db.backup() returns Promise<BackupMetadata> (async); concept cluster requires sync-shaped physical backup",
  would_close_with_cluster: "concept:sql-physical-backup",
})
class RefusedBackup {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/refuse.ts");

    expect(result.libraryBindings).toHaveLength(0);
    expect(result.libraryRefusals).toHaveLength(1);
    const refusal = result.libraryRefusals[0]!;
    expect(refusal.kind).toBe("refusal-memento");
    expect(refusal.target_language).toBe("typescript");
    expect(refusal.surface).toBe("typescript-bind");
    expect(refusal.concept).toBe("concept:sql-physical-backup");
    expect(refusal.reason).toContain("async");
    expect(refusal.would_close_with_cluster).toBe("concept:sql-physical-backup");
  });

  it("does not emit a refusal-memento for a plain class without @sugar.refuse (discrimination)", () => {
    const source = `
class PlainClass {
  method(): void {}
}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/plain-class.ts");

    expect(result.libraryRefusals).toHaveLength(0);
    expect(result.libraryBindings).toHaveLength(0);
  });

  it("does not emit a refusal-memento when required fields are missing from @sugar.refuse (structural discrimination)", () => {
    const source = `
import { sugar } from "provekit";

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-physical-backup",
})
class IncompleteRefusal {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/incomplete-refuse.ts");

    // Missing reason and would_close_with_cluster: no refusal emitted
    expect(result.libraryRefusals).toHaveLength(0);
  });

  it("does not emit a refusal-memento for a @sugar.bind class decorator (wrong decorator discrimination)", () => {
    const source = `
import { sugar } from "provekit";

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
class WrongDecorator {}
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/wrong-dec.ts");

    expect(result.libraryRefusals).toHaveLength(0);
    expect(result.libraryBindings).toHaveLength(0);
  });

  it("emits libraryRefusals that are distinct from structural lift-time refusals", () => {
    const source = `
import { sugar } from "provekit";

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:dynamic-library-load",
  reason: "loadExtension() is an OS-tier binding; not a SQL-driver concern",
  would_close_with_cluster: "concept:dynamic-library-load",
})
class RefusedLoadExtension {}

@sugar.bind({ concept: "concept:sql-query", library: "better-sqlite3" })
function queryAll(db: unknown): unknown[] { return []; }
`;
    const result = liftTypeScriptLibraryBindingsText(source, "src/mixed.ts");

    // Sugar bindings go into libraryBindings; refusals go into libraryRefusals
    expect(result.libraryBindings).toHaveLength(1);
    expect(result.libraryRefusals).toHaveLength(1);
    // Structural lift-time refusals (for unsupported syntax) remain separate
    expect(result.refusals).toHaveLength(0);
  });
});
