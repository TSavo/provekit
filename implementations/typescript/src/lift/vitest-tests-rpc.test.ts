import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { liftVitestTestsIrDocument } from "./vitest-tests-rpc.js";

function tempDir(): string {
  return mkdtempSync(join(tmpdir(), "provekit-ts-vitest-rpc-"));
}

describe("typescript vitest lift RPC projection", () => {
  it("exposes existing Vitest assertion lifting as ir-document contract entries", () => {
    const root = tempDir();
    mkdirSync(join(root, "src"));
    writeFileSync(
      join(root, "double.test.ts"),
      `
import { expect, it } from "vitest";
import { double } from "./src/double";

it("double three is six", () => {
  expect(double(3)).toBe(6);
});
`,
      "utf8",
    );

    const doc = liftVitestTestsIrDocument(root, ["."]);

    expect(doc.kind).toBe("ir-document");
    expect(doc.ir).toHaveLength(1);
    const contract = doc.ir[0] as any;
    expect(contract.kind).toBe("contract");
    expect(contract.name).toBe("double three is six::0");
    expect(contract.outBinding).toBe("out");
    expect(contract.inv).toMatchObject({
      kind: "atomic",
      name: "=",
      args: [
        { kind: "ctor", name: "double" },
        { kind: "const", value: 6, sort: { kind: "primitive", name: "Int" } },
      ],
    });
    expect(contract.inv.args[0].args).toHaveLength(1);
  });
});
