/**
 * writeAttestSummary Action tests.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, readFileSync, existsSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeWriteAttestSummaryAction,
  WRITE_ATTEST_SUMMARY_CAPABILITY,
} from "./writeAttestSummary.js";
import type { VerifyProjectInvariantsStageOutput } from "./verifyProjectInvariants.js";

function makeFakeSummary(): VerifyProjectInvariantsStageOutput {
  return {
    declarations: [
      {
        declarationName: "foo",
        filePath: "src/a.invariant.ts",
        cid: "bafy-decl-1",
        bindingHash: "bb1",
        propertyHash: "pp1",
        declarationKind: "property",
      },
    ],
    projectRootCid: "bafy-root-1",
    nullRoots: ["bafy-missing-1"],
  };
}

describe("writeAttestSummary", () => {
  it("exposes the canonical capability name", () => {
    expect(WRITE_ATTEST_SUMMARY_CAPABILITY).toBe("write-attest-summary");
  });

  it("writes the summary as pretty-printed JSON to the requested path", async () => {
    const action = makeWriteAttestSummaryAction();
    const tmp = mkdtempSync(join(tmpdir(), "attest-summary-"));
    const outPath = join(tmp, "nested", "summary.json");
    const summary = makeFakeSummary();

    const resource = await action.run({
      summary,
      projectName: "p",
      projectVersion: "0.1.0",
      outPath,
    });

    expect(existsSync(outPath)).toBe(true);
    expect(resource.path).toBe(outPath);
    expect(resource.projectRootCid).toBe("bafy-root-1");
    expect(resource.nullRootCount).toBe(1);

    const content = JSON.parse(readFileSync(outPath, "utf-8"));
    expect(content.projectName).toBe("p");
    expect(content.projectVersion).toBe("0.1.0");
    expect(content.projectRootCid).toBe("bafy-root-1");
    expect(content.declarations).toEqual(summary.declarations);
    expect(content.nullRoots).toEqual(summary.nullRoots);
  });

  it("rejects relative paths", async () => {
    const action = makeWriteAttestSummaryAction();
    await expect(
      action.run({
        summary: makeFakeSummary(),
        projectName: "p",
        projectVersion: "v",
        outPath: "relative.json",
      }),
    ).rejects.toThrow(/must be absolute/);
  });

  it("Action shape: describeResource includes path/cid/null-root count", () => {
    const action = makeWriteAttestSummaryAction();
    const desc = action.describeResource({
      path: "/tmp/s.json",
      contentSha256: "deadbeef",
      bytesWritten: 99,
      projectRootCid: "bafy-rrr",
      nullRootCount: 3,
    });
    expect(desc).toMatch(/wrote attest summary/);
    expect(desc).toMatch(/nullRoots=3/);
    expect(desc).toMatch(/projectRoot=bafy-rrr/);
  });

  it("Action shape: serializeInput summarizes the input for audit", () => {
    const action = makeWriteAttestSummaryAction();
    expect(
      action.serializeInput({
        summary: makeFakeSummary(),
        projectName: "p",
        projectVersion: "v",
        outPath: "/abs/s.json",
      }),
    ).toEqual({
      path: "/abs/s.json",
      projectName: "p",
      projectVersion: "v",
      projectRootCid: "bafy-root-1",
      declarationCount: 1,
      nullRootCount: 1,
    });
  });
});
