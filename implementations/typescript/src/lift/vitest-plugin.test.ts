import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, writeFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import provekitLiftPlugin, { runLiftOnce } from "./vitest-plugin.js";
import { publicKeyForSeed, DEFAULT_LIFT_SEED } from "./index.js";
import { verifyProofEnvelope } from "../proofEnvelope/index.js";

function tempDir(): string {
  return mkdtempSync(join(tmpdir(), "provekit-lift-plug-"));
}

describe("vitest-plugin / runLiftOnce", () => {
  let savedCwd = "";
  beforeEach(() => {
    savedCwd = process.cwd();
  });
  afterEach(() => {
    process.chdir(savedCwd);
    delete process.env.PROVEKIT_LIFT_STRICT;
  });

  it("walks the workspace, mints a .proof, and reports per-adapter counts", () => {
    const td = tempDir();
    const src = join(td, "src");
    mkdirSync(src);
    writeFileSync(
      join(src, "dto.ts"),
      `import { IsNotEmpty, IsEmail, Min, Max } from "class-validator";
       export class UserDto {
         @IsNotEmpty() name: string;
         @IsEmail() email: string;
         @Min(0) @Max(120) age: number;
       }
      `,
    );
    writeFileSync(
      join(src, "schema.ts"),
      `import { z } from "zod";
       export const Port = z.number().int().min(1).max(65535);
      `,
    );

    const result = runLiftOnce({
      workspace: src,
      outDir: join(td, "out"),
      silent: true,
    });

    expect(result.totalLifted).toBe(2);
    expect(result.cid.startsWith("blake3-512:")).toBe(true);
    expect(existsSync(result.outPath)).toBe(true);
    expect(result.outPath.endsWith(".proof")).toBe(true);
    expect(result.perAdapter["zod"]!.lifted).toBe(1);
    expect(result.perAdapter["class-validator"]!.lifted).toBe(1);
    expect(result.perAdapter["fast-check"]!.lifted).toBe(0);
    expect(result.summary).toContain("ProvekIt: lifted 2 contracts");
    expect(result.summary).toContain("1 zod");
    expect(result.summary).toContain("1 class-validator");
  });

  it("strict mode throws when adapter warnings are present", () => {
    const td = tempDir();
    const src = join(td, "src");
    mkdirSync(src);
    writeFileSync(
      join(src, "dto.ts"),
      `import { IsNotEmpty } from "class-validator";
       export class W { @IsNotEmpty() @Unknown() x: string; }
      `,
    );
    expect(() =>
      runLiftOnce({ workspace: src, outDir: join(td, "out"), strict: true, silent: true }),
    ).toThrow(/strict mode.*skipped/);
  });

  it("strict mode honors PROVEKIT_LIFT_STRICT env var when option not set", () => {
    const td = tempDir();
    const src = join(td, "src");
    mkdirSync(src);
    // No liftable contracts — strict should reject.
    writeFileSync(join(src, "empty.ts"), `export const x = 1;`);
    process.env.PROVEKIT_LIFT_STRICT = "1";
    expect(() =>
      runLiftOnce({ workspace: src, outDir: join(td, "out"), silent: true }),
    ).toThrow(/strict mode.*no liftable/);
  });

  it("loose mode tolerates a workspace with zero liftable contracts", () => {
    const td = tempDir();
    const src = join(td, "src");
    mkdirSync(src);
    writeFileSync(join(src, "empty.ts"), `export const x = 1;`);
    const result = runLiftOnce({
      workspace: src,
      outDir: join(td, "out"),
      silent: true,
    });
    expect(result.totalLifted).toBe(0);
    expect(result.cid).toBe("");
  });

  it("the minted .proof from runLiftOnce verifies via verifyProofEnvelope", () => {
    const td = tempDir();
    const src = join(td, "src");
    mkdirSync(src);
    writeFileSync(
      join(src, "schema.ts"),
      `import { z } from "zod";
       export const Name = z.string().nonempty();
      `,
    );
    const result = runLiftOnce({
      workspace: src,
      outDir: join(td, "out"),
      silent: true,
    });
    const bytes = readFileSync(result.outPath);
    const verified = verifyProofEnvelope(
      new Uint8Array(bytes),
      result.cid,
      publicKeyForSeed(DEFAULT_LIFT_SEED),
    );
    expect(verified.ok).toBe(true);
    expect(verified.errors).toEqual([]);
  });
});

describe("vitest-plugin / plugin object shape", () => {
  it("returns a plugin object with name=provekit-lift, enforce=pre, and a buildStart hook", () => {
    const p = provekitLiftPlugin({ silent: true });
    expect(p.name).toBe("provekit-lift");
    expect(p.enforce).toBe("pre");
    expect(typeof p.buildStart).toBe("function");
    expect(p.__lastRun).toBeNull();
  });

  it("buildStart populates __lastRun with the run result", () => {
    const td = tempDir();
    const src = join(td, "src");
    mkdirSync(src);
    writeFileSync(
      join(src, "schema.ts"),
      `import { z } from "zod";
       export const Tag = z.string().nonempty();
      `,
    );
    const p = provekitLiftPlugin({
      workspace: src,
      outDir: join(td, "out"),
      silent: true,
    });
    p.buildStart();
    expect(p.__lastRun).not.toBeNull();
    expect(p.__lastRun!.totalLifted).toBe(1);
  });
});
