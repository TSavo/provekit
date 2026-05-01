import { describe, it, expect } from "vitest";
import { mkdtempSync, writeFileSync, mkdirSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  liftPath,
  mintProof,
  liftAndMint,
  defaultLiftOptions,
  publicKeyForSeed,
  DEFAULT_LIFT_SEED,
} from "./index.js";
import { verifyProofEnvelope } from "../proofEnvelope/index.js";

const FIXTURE_DIR = join(__dirname, "__fixtures__");

function tempDir(prefix = "provekit-lift-ts-"): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

describe("lift / zod adapter", () => {
  it("lifts top-level z.object schemas with int/min/email/uuid/positive shapes", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "user.ts"),
      `import { z } from "zod";
       export const UserSchema = z.object({
         age: z.number().int().nonnegative(),
         email: z.string().min(1).email(),
       });
       export const Port = z.number().int().min(1).max(65535);
      `,
    );
    const r = liftPath(td);
    const zodReport = r.adapterReports.find((a) => a.adapter === "zod")!;
    expect(zodReport.lifted).toBe(2);
    expect(r.decls.map((d) => d.name).sort()).toEqual(["Port", "UserSchema"]);
    const user = r.decls.find((d) => d.name === "UserSchema")!;
    expect(user.pre).toBeDefined();
    expect(user.adapter).toBe("zod");
  });

  it("skips zod schemas that use .refine(callback) with a warning", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "skip.ts"),
      `import { z } from "zod";
       export const Bad = z.string().refine((s) => s.length > 3);
      `,
    );
    const r = liftPath(td);
    const zodReport = r.adapterReports.find((a) => a.adapter === "zod")!;
    expect(zodReport.seen).toBe(1);
    expect(zodReport.lifted).toBe(0);
    expect(zodReport.warnings).toHaveLength(1);
    expect(zodReport.warnings[0]!.reason).toMatch(/refine/);
  });
});

describe("lift / fast-check adapter", () => {
  it("lifts fc.assert(fc.property(...)) inside it(...) into a forall contract", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "props.ts"),
      `import * as fc from "fast-check";
       it("nat is nonneg", () => {
         fc.assert(fc.property(fc.nat(), (n) => n >= 0));
       });
      `,
    );
    const r = liftPath(td);
    const fcReport = r.adapterReports.find((a) => a.adapter === "fast-check")!;
    expect(fcReport.lifted).toBe(1);
    const decl = r.decls[0]!;
    expect(decl.adapter).toBe("fast-check");
    expect(decl.inv).toBeDefined();
    expect(decl.inv!.kind).toBe("forall");
  });

  it("skips fast-check property when predicate body is multi-statement", () => {
    const td = tempDir();
    writeFileSync(
      join(td, "skip.ts"),
      `import * as fc from "fast-check";
       it("complex", () => {
         fc.assert(fc.property(fc.integer(), (x) => {
           const y = x * 2;
           return y > x;
         }));
       });
      `,
    );
    const r = liftPath(td);
    const fcReport = r.adapterReports.find((a) => a.adapter === "fast-check")!;
    expect(fcReport.seen).toBe(1);
    expect(fcReport.lifted).toBe(0);
    expect(fcReport.warnings[0]!.reason).toMatch(/single expression|comparison/);
  });
});

describe("lift / fixture end-to-end", () => {
  it("lifts >=N contracts from the bundled mixed fixture and produces a deterministic .proof", () => {
    const r1 = liftPath(FIXTURE_DIR);
    const zod = r1.adapterReports.find((a) => a.adapter === "zod")!;
    const fcA = r1.adapterReports.find((a) => a.adapter === "fast-check")!;

    expect(zod.lifted).toBeGreaterThanOrEqual(5);
    expect(zod.warnings.length).toBeGreaterThanOrEqual(1); // refine
    expect(fcA.lifted).toBeGreaterThanOrEqual(3);
    expect(fcA.warnings.length).toBeGreaterThanOrEqual(1); // multi-stmt body
    expect(r1.decls.length).toBeGreaterThan(3);

    const opts = defaultLiftOptions();
    const minted = mintProof(r1.decls, opts);
    expect(minted.cid.startsWith("blake3-512:")).toBe(true);
    expect(minted.memberCount).toBe(r1.decls.length);

    // CIDs are deterministic across runs given fixed seed and producedAt.
    const r2 = liftPath(FIXTURE_DIR);
    const minted2 = mintProof(r2.decls, opts);
    expect(minted2.cid).toBe(minted.cid);
  });

  it("liftAndMint writes <cid>.proof and the file round-trips through verifyProofEnvelope", () => {
    const td = tempDir();
    const out = liftAndMint(FIXTURE_DIR, td);
    expect(existsSync(out.outPath)).toBe(true);
    const bytes = readFileSync(out.outPath);
    const verified = verifyProofEnvelope(
      new Uint8Array(bytes),
      out.minted.cid,
      publicKeyForSeed(DEFAULT_LIFT_SEED),
    );
    expect(verified.errors).toEqual([]);
    expect(verified.ok).toBe(true);
    expect(verified.catalog).not.toBeNull();
    expect(verified.catalog!.members.size).toBe(out.minted.memberCount);
  });

  it("CLI runs end-to-end on the fixture directory", async () => {
    const { runCli, parseCliArgs } = await import("./cli.js");
    const td = tempDir();
    const exitCode = runCli(parseCliArgs([FIXTURE_DIR, "--out", td, "--quiet"]));
    expect(exitCode).toBe(0);
  });
});

describe("lift / dedup", () => {
  it("collapses two files with the same schema to one minted member", () => {
    const td = tempDir();
    mkdirSync(join(td, "a"));
    mkdirSync(join(td, "b"));
    const schema =
      `import { z } from "zod"; export const Same = z.string().min(1);`;
    writeFileSync(join(td, "a", "x.ts"), schema);
    writeFileSync(join(td, "b", "y.ts"), schema);
    const r = liftPath(td);
    expect(r.decls.length).toBe(2);
    const minted = mintProof(r.decls, defaultLiftOptions());
    expect(minted.deduplicated).toBeGreaterThanOrEqual(1);
    expect(minted.memberCount).toBe(1);
  });
});
