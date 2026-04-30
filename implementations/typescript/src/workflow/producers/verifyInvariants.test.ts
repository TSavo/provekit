/**
 * verifyInvariants Stage tests. Covers Stage shape + a no-invariant
 * smoke run; full Z3 path verification is exercised by the runtime
 * suite.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeVerifyInvariantsStage,
  runVerifyInvariants,
  VERIFY_INVARIANTS_CAPABILITY,
} from "./verifyInvariants.js";

describe("verifyInvariants", () => {
  it("exposes the canonical capability name", () => {
    expect(VERIFY_INVARIANTS_CAPABILITY).toBe("verify-invariants");
  });

  it("returns zero/empty report against a project with no invariants", async () => {
    const projectRoot = mkdtempSync(join(tmpdir(), "verify-inv-stage-"));
    const out = await runVerifyInvariants({
      projectRoot,
      adversarial: false,
    });
    expect(out.summary.total).toBe(0);
    expect(out.verdicts).toEqual([]);
    expect(out.exitCode).toBe(0);
  });

  it("Stage shape: serializeInput drops undefined optionals", () => {
    const stage = makeVerifyInvariantsStage();
    expect(
      stage.serializeInput({
        projectRoot: "/p",
        adversarial: true,
      }),
    ).toEqual({ projectRoot: "/p", adversarial: true });
    expect(
      stage.serializeInput({
        projectRoot: "/p",
        adversarial: false,
        timeoutMs: 30000,
        maxPaths: 100,
      }),
    ).toEqual({
      projectRoot: "/p",
      adversarial: false,
      timeoutMs: 30000,
      maxPaths: 100,
    });
  });

  it("Stage shape: round-trips output through serialize/deserialize", () => {
    const stage = makeVerifyInvariantsStage();
    const out = {
      verdicts: [
        {
          invariantId: "inv-1",
          scope: "callsite" as const,
          status: "holds" as const,
          pathCheck: "holds" as const,
          cacheStatus: "miss" as const,
        },
      ],
      summary: {
        total: 1,
        holds: 1,
        decayed: 0,
        violated: 0,
        cacheHits: 0,
        cacheMisses: 1,
      },
      exitCode: 0,
    };
    expect(stage.deserializeOutput(stage.serializeOutput(out))).toEqual(out);
  });
});
