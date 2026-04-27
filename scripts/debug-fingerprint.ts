#!/usr/bin/env tsx
import { Project } from "ts-morph";
import { nodeFingerprint, buildPrint } from "../src/sast/fingerprint.js";

function fingerprintFn(source: string): string {
  const project = new Project({ useInMemoryFileSystem: true });
  const file = project.createSourceFile("test.ts", source);
  const fn = file.getFirstDescendant((n) => n.getKindName() === "FunctionDeclaration");
  if (!fn) throw new Error("no FunctionDeclaration");
  return nodeFingerprint(fn);
}

const cases: Array<{ name: string; a: string; b: string; expect: "same" | "diff" }> = [
  {
    name: "idempotent",
    a: `function f(a: number, b: number): number { return a + b; }`,
    b: `function f(a: number, b: number): number { return a + b; }`,
    expect: "same",
  },
  {
    name: "whitespace differences",
    a: `function f(a: number, b: number): number { return a + b; }`,
    b: `function f(a:number,b:number):number{return a+b;}`,
    expect: "same",
  },
  {
    name: "comment differences",
    a: `function f(a: number): number { return a + 1; }`,
    b: `// commented version
function f(a: number): number {
  // body comment
  return a + 1;
}`,
    expect: "same",
  },
  {
    name: "renamed identifier",
    a: `function f(a: number): number { return a + 1; }`,
    b: `function g(a: number): number { return a + 1; }`,
    expect: "diff",
  },
  {
    name: "changed literal",
    a: `function f(a: number): number { return a + 1; }`,
    b: `function f(a: number): number { return a + 2; }`,
    expect: "diff",
  },
  {
    name: "changed operator",
    a: `function f(a: number, b: number): number { return a + b; }`,
    b: `function f(a: number, b: number): number { return a - b; }`,
    expect: "diff",
  },
  {
    name: "added clause (division-by-zero target)",
    a: `function f(a: number, b: number): number { return a / b; }`,
    b: `function f(a: number, b: number): number {
  if (b === 0) throw new Error("Division by zero");
  return a / b;
}`,
    expect: "diff",
  },
  {
    name: "added OR-chain clause",
    a: `function f(parent: any): boolean {
  return parent.type === "Foo" || parent.type === "Bar";
}`,
    b: `function f(parent: any): boolean {
  return parent.type === "Foo" || parent.type === "Bar" || parent.type === "Baz";
}`,
    expect: "diff",
  },
];

let pass = 0;
let fail = 0;
for (const c of cases) {
  const fa = fingerprintFn(c.a);
  const fb = fingerprintFn(c.b);
  const same = fa === fb;
  const ok = c.expect === "same" ? same : !same;
  console.log(`${ok ? "PASS" : "FAIL"}  ${c.name}  expect=${c.expect}  same=${same}`);
  if (!ok) {
    console.log(`  a fingerprint: ${fa}`);
    console.log(`  b fingerprint: ${fb}`);
    if (process.env["DEBUG"]) {
      // Re-run with buildPrint to see structural difference.
      const project = new Project({ useInMemoryFileSystem: true });
      const fileA = project.createSourceFile("a.ts", c.a);
      const fileB = project.createSourceFile("b.ts", c.b);
      const fnA = fileA.getFirstDescendant((n) => n.getKindName() === "FunctionDeclaration")!;
      const fnB = fileB.getFirstDescendant((n) => n.getKindName() === "FunctionDeclaration")!;
      console.log(`  a buildPrint: ${buildPrint(fnA)}`);
      console.log(`  b buildPrint: ${buildPrint(fnB)}`);
    }
    fail++;
  } else {
    pass++;
  }
}
console.log();
console.log(`${pass}/${pass + fail} passed`);
process.exit(fail > 0 ? 1 : 0);
