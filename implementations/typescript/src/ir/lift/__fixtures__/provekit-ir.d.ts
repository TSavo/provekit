/**
 * Stub declarations for the SURFACE form of `provekit/ir` and
 * `provekit/sorts`, used to make test fixtures type-check under
 * tsc.createProgram. The lifter never reads these: it works at
 * the AST level: but the TypeChecker needs them to resolve
 * imports without flooding diagnostics.
 *
 * The shape mirrors spec §7 and §5.
 */

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
