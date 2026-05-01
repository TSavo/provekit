import { describe, it, expect } from "vitest";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { liftPath, mintProof, defaultLiftOptions } from "./index.js";

function tempDir(): string {
  return mkdtempSync(join(tmpdir(), "provekit-lift-cv-"));
}

describe("lift / class-validator adapter", () => {
  it("lifts a CreateUserDto with IsNotEmpty/MinLength/IsEmail/Min/Max into a single contract", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "dto.ts"),
      `import { IsNotEmpty, MinLength, IsEmail, Min, Max } from "class-validator";
       export class CreateUserDto {
         @IsNotEmpty() @MinLength(2) username: string;
         @IsEmail() email: string;
         @Min(0) @Max(120) age: number;
       }
      `,
    );
    const r = liftPath(td);
    const cv = r.adapterReports.find((a) => a.adapter === "class-validator")!;
    expect(cv.seen).toBe(1);
    expect(cv.lifted).toBe(1);
    expect(cv.warnings).toHaveLength(0);
    const decl = r.decls.find((d) => d.name === "CreateUserDto")!;
    expect(decl).toBeDefined();
    expect(decl.adapter).toBe("class-validator");
    expect(decl.pre).toBeDefined();
    expect(decl.pre!.kind).toBe("forall");
  });

  it("ignores classes with no decorated properties", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "plain.ts"),
      `export class Plain { foo: string; bar: number; }`,
    );
    const r = liftPath(td);
    const cv = r.adapterReports.find((a) => a.adapter === "class-validator")!;
    expect(cv.seen).toBe(0);
    expect(cv.lifted).toBe(0);
  });

  it("skips a class with an unknown decorator and surfaces a warning", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "weird.ts"),
      `import { IsNotEmpty } from "class-validator";
       export class WeirdDto {
         @IsNotEmpty() @CustomMagicCheck() field: string;
       }
      `,
    );
    const r = liftPath(td);
    const cv = r.adapterReports.find((a) => a.adapter === "class-validator")!;
    expect(cv.seen).toBe(1);
    expect(cv.lifted).toBe(0);
    expect(cv.warnings).toHaveLength(1);
    expect(cv.warnings[0]!.reason).toMatch(/CustomMagicCheck|unsupported/);
  });

  it("encodes IsEmail as a kit-Ctor matches_email_regex predicate", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "email.ts"),
      `import { IsEmail } from "class-validator";
       export class EmailDto { @IsEmail() addr: string; }`,
    );
    const r = liftPath(td);
    const decl = r.decls.find((d) => d.name === "EmailDto")!;
    const json = JSON.stringify(decl.pre);
    expect(json).toContain("matches_email_regex");
  });

  it("Length(min, max) emits both >= and <= length conjuncts", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "len.ts"),
      `import { Length } from "class-validator";
       export class LenDto { @Length(3, 10) v: string; }`,
    );
    const r = liftPath(td);
    const decl = r.decls.find((d) => d.name === "LenDto")!;
    const json = JSON.stringify(decl.pre);
    expect(json).toContain("\"length\"");
    expect(json).toContain("\">=\"");
    expect(json).toContain("\"<=\"");
  });
});

describe("lift / mixedAllAdapters fixture", () => {
  it("lifts contracts from all three adapters in one file", () => {
    const fixtureDir = join(__dirname, "__fixtures__");
    const r = liftPath(fixtureDir);
    const zod = r.adapterReports.find((a) => a.adapter === "zod")!;
    const fcA = r.adapterReports.find((a) => a.adapter === "fast-check")!;
    const cv = r.adapterReports.find((a) => a.adapter === "class-validator")!;
    expect(zod.lifted).toBeGreaterThanOrEqual(3);
    expect(fcA.lifted).toBeGreaterThanOrEqual(2);
    expect(cv.lifted).toBeGreaterThanOrEqual(3);
    expect(cv.warnings.length).toBeGreaterThanOrEqual(1);
    const minted = mintProof(r.decls, defaultLiftOptions());
    expect(minted.cid.startsWith("blake3-512:")).toBe(true);
  });
});
