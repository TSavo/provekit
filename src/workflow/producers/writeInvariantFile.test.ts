import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  writeInvariantFile,
  makeWriteInvariantFileAction,
  WRITE_INVARIANT_FILE_CAPABILITY,
} from "./writeInvariantFile.js";

let tmp: string;

beforeEach(() => {
  tmp = mkdtempSync(join(tmpdir(), "writeInvariant-test-"));
});

afterEach(() => {
  if (existsSync(tmp)) rmSync(tmp, { recursive: true, force: true });
});

describe("writeInvariantFile", () => {
  it("writes <basename>.invariant.ts next to the target file", () => {
    const target = join(tmp, "src/billing/invoice.ts");
    const handle = writeInvariantFile({
      targetFile: target,
      surfaceText: 'import { must } from "provekit/ir/symbolic";\n',
    });
    expect(handle.invariantFilePath).toBe(join(tmp, "src/billing/invoice.invariant.ts"));
    expect(existsSync(handle.invariantFilePath)).toBe(true);
    expect(handle.bytesWritten).toBeGreaterThan(0);
    expect(handle.preExisting).toBe(false);
  });

  it("strips .ts/.tsx suffix when computing the invariant filename", () => {
    const handle1 = writeInvariantFile({
      targetFile: join(tmp, "src/foo.ts"),
      surfaceText: "// a",
    });
    expect(handle1.invariantFilePath).toBe(join(tmp, "src/foo.invariant.ts"));

    const handle2 = writeInvariantFile({
      targetFile: join(tmp, "src/Bar.tsx"),
      surfaceText: "// b",
    });
    expect(handle2.invariantFilePath).toBe(join(tmp, "src/Bar.invariant.ts"));
  });

  it("overwrites by default", () => {
    const target = join(tmp, "src/foo.ts");
    const path = join(tmp, "src/foo.invariant.ts");

    writeInvariantFile({ targetFile: target, surfaceText: "// first\n" });
    const handle = writeInvariantFile({
      targetFile: target,
      surfaceText: "// second\n",
    });

    expect(handle.preExisting).toBe(true);
    expect(readFileSync(path, "utf8")).toBe("// second\n");
  });

  it("appends with --append=true", () => {
    const target = join(tmp, "src/foo.ts");
    const path = join(tmp, "src/foo.invariant.ts");

    writeInvariantFile({ targetFile: target, surfaceText: "// first\n" });
    const handle = writeInvariantFile({
      targetFile: target,
      surfaceText: "// second\n",
      append: true,
    });

    expect(handle.preExisting).toBe(true);
    const content = readFileSync(path, "utf8");
    expect(content).toContain("// first");
    expect(content).toContain("// second");
  });

  it("contentHash is sha256 hex of the final content", () => {
    const handle = writeInvariantFile({
      targetFile: join(tmp, "src/foo.ts"),
      surfaceText: "abc",
    });
    expect(handle.contentHash).toMatch(/^[0-9a-f]{64}$/);
  });
});

describe("makeWriteInvariantFileAction", () => {
  it("exposes the expected capability constant", () => {
    expect(WRITE_INVARIANT_FILE_CAPABILITY).toBe("write-invariant-file");
  });

  it("returns an Action with the right shape", () => {
    const action = makeWriteInvariantFileAction();
    expect(action.name).toBe("writeInvariantFile");
    expect(action.producedBy).toMatch(/writeInvariantFile/);
    expect(typeof action.run).toBe("function");
    expect(typeof action.serializeInput).toBe("function");
    expect(typeof action.describeResource).toBe("function");
  });

  it("serializeInput returns content-defining fields", () => {
    const action = makeWriteInvariantFileAction();
    const serialized = action.serializeInput({
      targetFile: "/some/path.ts",
      surfaceText: "// content",
    }) as { targetFile: string; surfaceText: string; append: boolean };
    expect(serialized.targetFile).toBe("/some/path.ts");
    expect(serialized.surfaceText).toBe("// content");
    expect(serialized.append).toBe(false);
  });

  it("describeResource returns human-readable resource description", () => {
    const action = makeWriteInvariantFileAction();
    const desc = action.describeResource({
      invariantFilePath: "/some/path.invariant.ts",
      contentHash: "deadbeef".repeat(8),
      bytesWritten: 42,
      preExisting: false,
    });
    expect(desc).toContain("42 bytes");
    expect(desc).toContain("/some/path.invariant.ts");
    expect(desc).toContain("deadbeef".slice(0, 16));
  });

  it("Action.run() actually writes the file", async () => {
    const action = makeWriteInvariantFileAction();
    const handle = await action.run({
      targetFile: join(tmp, "src/bar.ts"),
      surfaceText: "import { must } from 'provekit/ir/symbolic';\n",
    });
    expect(existsSync(handle.invariantFilePath)).toBe(true);
  });
});
