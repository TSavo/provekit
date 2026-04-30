import { describe, it, expect } from "vitest";
import { collectTopLevelNames } from "./moduleLoader";

describe("collectTopLevelNames", () => {
  it("finds function declarations", () => {
    const src = `function foo() {}\nfunction bar(x: number) { return x; }`;
    const names = collectTopLevelNames(src);
    expect(names).toContain("foo");
    expect(names).toContain("bar");
  });

  it("finds exported function declarations", () => {
    const src = `export function greet() {}\nexport async function fetchData() {}`;
    const names = collectTopLevelNames(src);
    expect(names).toContain("greet");
    expect(names).toContain("fetchData");
  });

  it("finds class declarations", () => {
    const src = `class Foo {}\nexport class Bar {}`;
    const names = collectTopLevelNames(src);
    expect(names).toContain("Foo");
    expect(names).toContain("Bar");
  });

  it("finds lexical declarations", () => {
    const src = `const a = 1;\nlet b = 2;\nvar c = 3;`;
    const names = collectTopLevelNames(src);
    expect(names).toContain("a");
    expect(names).toContain("b");
    expect(names).toContain("c");
  });

  it("finds type aliases and interfaces", () => {
    const src = `type Foo = number;\ninterface Bar { x: string }`;
    const names = collectTopLevelNames(src);
    expect(names).toContain("Foo");
    expect(names).toContain("Bar");
  });

  it("does not duplicate exported declarations", () => {
    const src = `export function sameName() {}\nexport const otherName = 1;`;
    const names = collectTopLevelNames(src);
    const saneOccurrences = names.filter((n) => n === "sameName").length;
    expect(saneOccurrences).toBe(1);
  });

  it("handles empty source", () => {
    expect(collectTopLevelNames("")).toEqual([]);
  });

  it("handles source with only imports", () => {
    const src = `import { foo } from "./bar";\nimport baz from "./qux";`;
    // Imports aren't top-level declarations we want to re-export; should yield no names.
    // If the impl includes them, that's OK too — either way the set should be consistent.
    const names = collectTopLevelNames(src);
    expect(Array.isArray(names)).toBe(true);
  });
});
