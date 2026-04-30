/**
 * Lift a string of TS-IR-language surface text into an IrFormula.
 *
 * The formulate stage's LLM emits `.invariant.ts` source as a string;
 * this helper materializes that string into a synthetic `ts.Program`
 * and runs `liftProject` against it. The virtual file is given a
 * `.invariant.ts` extension so the lifter's anchoring check (spec §3)
 * permits `property()` calls inside it.
 *
 * Spec: protocol/specs/2026-04-29-ts-ir-language.md §9 (the lifter)
 *
 * The helper bundles ambient declarations for `provekit/ir` and
 * `provekit/sorts` so surface text that imports those modules
 * type-checks under tsc without a real dependency resolution pass.
 * The lifter itself doesn't read declaration files; it walks AST
 * nodes. The declarations exist only to keep the type checker quiet.
 */
import path from "node:path";
import ts from "typescript";
import type { IrFormula } from "../formulas.js";
import { liftProject, type LiftDiagnostic } from "./index.js";
import type { LiftedProperty } from "./visitor.js";

export interface LiftSurfaceTextResult {
  properties: LiftedProperty[];
  diagnostics: LiftDiagnostic[];
}

const PROVEKIT_IR_DTS = `
declare module "provekit/ir" {
  export type Int = number & { readonly __sort: "Int" };
  export type Real = number & { readonly __sort: "Real" };
  export type Bool = boolean & { readonly __sort: "Bool" };
  export type StringSort = string & { readonly __sort: "String" };

  export const Int: Int;
  export const Real: Real;
  export const Bool: Bool;
  export const StringSort: StringSort;

  export function property(name: string, formula: boolean): void;
  export function property(name: string, formula: () => boolean): void;
  export function assert(formula: boolean): void;
  export function forAll<T>(predicate: (x: T) => boolean): boolean;
  export function exists<T>(predicate: (x: T) => boolean): boolean;
  export function implies(antecedent: boolean, consequent: boolean): boolean;
  export function iff(left: boolean, right: boolean): boolean;
  export function ref(name: string): boolean;
}

declare module "provekit/sorts" {
  export type Int = number & { readonly __sort: "Int" };
  export type Real = number & { readonly __sort: "Real" };
  export type Bool = boolean & { readonly __sort: "Bool" };
  export type StringSort = string & { readonly __sort: "String" };
}
`;

const VIRTUAL_DIR = "/__provekit_virtual__";
const PROVEKIT_DTS_PATH = path.join(VIRTUAL_DIR, "provekit-ir.d.ts");

/**
 * Lift the supplied surface text and return every property the lifter
 * recognized. The caller chooses what to do with multi-property output;
 * the formulate stage, for v1, picks the first.
 *
 * @param surfaceText TS-IR-language source — typically the LLM output.
 * @param virtualFilePath Where the surface text appears in the synthetic
 *   program. Must end in `.invariant.ts` to satisfy the lifter's anchoring
 *   rule. Defaults to `<VIRTUAL_DIR>/synthesized.invariant.ts`.
 */
export function liftSurfaceText(
  surfaceText: string,
  virtualFilePath: string = path.join(VIRTUAL_DIR, "synthesized.invariant.ts"),
): LiftSurfaceTextResult {
  if (!virtualFilePath.endsWith(".invariant.ts")) {
    throw new Error(
      `liftSurfaceText: virtualFilePath must end in .invariant.ts (got "${virtualFilePath}")`,
    );
  }

  const fileMap = new Map<string, string>([
    [PROVEKIT_DTS_PATH, PROVEKIT_IR_DTS],
    [virtualFilePath, surfaceText],
  ]);

  const compilerOptions: ts.CompilerOptions = {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    skipLibCheck: true,
    noEmit: true,
    esModuleInterop: true,
  };

  const host = ts.createCompilerHost(compilerOptions, true);
  const realGetSourceFile = host.getSourceFile.bind(host);
  host.getSourceFile = (fileName, languageVersion, onError, shouldCreateNewSourceFile) => {
    const override = fileMap.get(fileName);
    if (override !== undefined) {
      return ts.createSourceFile(fileName, override, languageVersion, true);
    }
    return realGetSourceFile(fileName, languageVersion, onError, shouldCreateNewSourceFile);
  };
  const realFileExists = host.fileExists.bind(host);
  host.fileExists = (fn) => fileMap.has(fn) || realFileExists(fn);
  const realReadFile = host.readFile.bind(host);
  host.readFile = (fn) => fileMap.get(fn) ?? realReadFile(fn);

  const program = ts.createProgram({
    rootNames: Array.from(fileMap.keys()),
    options: compilerOptions,
    host,
  });

  return liftProject(program);
}

export type { IrFormula };
