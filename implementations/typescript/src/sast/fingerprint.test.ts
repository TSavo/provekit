/**
 * Tests for AST node fingerprinting. The contract:
 *   - Same code ⇒ same fingerprint (idempotence)
 *   - Whitespace-only differences ⇒ same fingerprint
 *   - Comment-only differences ⇒ same fingerprint
 *   - Renamed identifiers ⇒ different fingerprint
 *   - Different literal values ⇒ different fingerprint
 *   - Different operators ⇒ different fingerprint
 *   - Structural changes (added clause, etc.) ⇒ different fingerprint
 */

import { describe, it, expect } from "vitest";
import { Project } from "ts-morph";
import { nodeFingerprint } from "./fingerprint.js";

function fingerprintFn(source: string): string {
  const project = new Project({ useInMemoryFileSystem: true });
  const file = project.createSourceFile("test.ts", source);
  // We fingerprint the FunctionDeclaration's body if present, else the
  // top-level Block / SourceFile. Tests below pin a specific shape.
  const fn = file.getFirstDescendant((n) => n.getKindName() === "FunctionDeclaration");
  if (!fn) throw new Error("no FunctionDeclaration in source");
  return nodeFingerprint(fn);
}

describe("nodeFingerprint", () => {
  it("idempotent: same source → same fingerprint", () => {
    const src = `function f(a: number, b: number): number { return a + b; }`;
    expect(fingerprintFn(src)).toBe(fingerprintFn(src));
  });

  it("ignores whitespace-only differences", () => {
    const a = `function f(a: number, b: number): number { return a + b; }`;
    const b = `function f(a:number,b:number):number{return a+b;}`;
    const c = `function f(a: number, b: number): number {\n  return a + b;\n}`;
    const fa = fingerprintFn(a);
    expect(fingerprintFn(b)).toBe(fa);
    expect(fingerprintFn(c)).toBe(fa);
  });

  it("ignores comment-only differences", () => {
    const a = `function f(a: number): number { return a + 1; }`;
    const b = `// commented version
function f(a: number): number {
  // body comment
  return a + 1;
}`;
    const c = `/** JSDoc */
function f(a: number): number { return a + 1; /* trailing */ }`;
    expect(fingerprintFn(a)).toBe(fingerprintFn(b));
    expect(fingerprintFn(a)).toBe(fingerprintFn(c));
  });

  it("differs on renamed identifier", () => {
    const a = `function f(a: number): number { return a + 1; }`;
    const b = `function g(a: number): number { return a + 1; }`;
    expect(fingerprintFn(a)).not.toBe(fingerprintFn(b));
  });

  it("differs on changed literal value", () => {
    const a = `function f(a: number): number { return a + 1; }`;
    const b = `function f(a: number): number { return a + 2; }`;
    expect(fingerprintFn(a)).not.toBe(fingerprintFn(b));
  });

  it("differs on changed operator", () => {
    const a = `function f(a: number, b: number): number { return a + b; }`;
    const b = `function f(a: number, b: number): number { return a - b; }`;
    expect(fingerprintFn(a)).not.toBe(fingerprintFn(b));
  });

  it("differs on added clause (the diff-aware mining target)", () => {
    const a = `function f(a: number, b: number): number { return a / b; }`;
    const b = `function f(a: number, b: number): number {
  if (b === 0) throw new Error("Division by zero");
  return a / b;
}`;
    expect(fingerprintFn(a)).not.toBe(fingerprintFn(b));
  });

  it("differs on added OR-chain clause (incomplete-enum-disjunction shape)", () => {
    const a = `function f(parent: any): boolean {
  return parent.type === "Foo" || parent.type === "Bar";
}`;
    const b = `function f(parent: any): boolean {
  return parent.type === "Foo" || parent.type === "Bar" || parent.type === "Baz";
}`;
    expect(fingerprintFn(a)).not.toBe(fingerprintFn(b));
  });

  it("differs on switched branch order (we don't normalize commutative ops)", () => {
    // This is a known limitation: a + b ≠ b + a in fingerprint, even though
    // they evaluate to the same value. Fine for our purpose — diff matching
    // shouldn't claim "same node" for swapped operands; that's an edit.
    const a = `function f(a: number, b: number): number { return a + b; }`;
    const b = `function f(a: number, b: number): number { return b + a; }`;
    expect(fingerprintFn(a)).not.toBe(fingerprintFn(b));
  });

  it("identical bodies under different surrounding context still fingerprint equal at the body level", () => {
    // Wrap the same function body inside two different containers; the
    // FunctionDeclaration nodes themselves differ because of the function
    // names, but the body Blocks should fingerprint identically.
    const project = new Project({ useInMemoryFileSystem: true });
    const file = project.createSourceFile(
      "test.ts",
      `
function f(a: number): number { return a + 1; }
function g(b: number): number { return b + 1; }
      `,
    );
    const fns = file.getFunctions();
    expect(fns).toHaveLength(2);
    // The bodies have different parameter names (a vs b) so the inner
    // Identifier nodes will differ → different fingerprint. This pins
    // the contract: identifier names ARE part of the fingerprint.
    expect(nodeFingerprint(fns[0]!.getBody()!)).not.toBe(
      nodeFingerprint(fns[1]!.getBody()!),
    );

    // But two functions with literally identical bodies (down to names)
    // should have identical body fingerprints.
    const file2 = project.createSourceFile(
      "test2.ts",
      `
function f(a: number): number { return a + 1; }
function h(a: number): number { return a + 1; }
      `,
    );
    const fns2 = file2.getFunctions();
    expect(nodeFingerprint(fns2[0]!.getBody()!)).toBe(
      nodeFingerprint(fns2[1]!.getBody()!),
    );
  });
});
