/**
 * writeMementoFile Action tests.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, readFileSync, existsSync } from "fs";
import { tmpdir } from "os";
import { join } from "path";
import {
  makeWriteMementoFileAction,
  WRITE_MEMENTO_FILE_CAPABILITY,
} from "./writeMementoFile.js";
import type { ClaimEnvelope } from "../../claimEnvelope/types.js";

function makeFakeEnvelope(): ClaimEnvelope {
  return {
    cid: "bafy-fake-cid-1",
    bindingHash: "aa",
    propertyHash: "bb",
    verdict: "holds",
    producedBy: "fake@v1",
    producedAt: "2026-01-01T00:00:00.000Z",
    inputCids: [],
    evidence: {
      kind: "legacy-witness",
      schema: "bafy-schema",
      body: { rawWitness: "{}", legacyProducerId: "fake" },
    },
    signature: "AAAA",
  } as unknown as ClaimEnvelope;
}

describe("writeMementoFile", () => {
  it("exposes the canonical capability name", () => {
    expect(WRITE_MEMENTO_FILE_CAPABILITY).toBe("write-memento-file");
  });

  it("writes the envelope as pretty-printed JSON to the requested path", async () => {
    const action = makeWriteMementoFileAction();
    const tmp = mkdtempSync(join(tmpdir(), "write-memento-"));
    const outPath = join(tmp, "nested", "memento.json");
    const envelope = makeFakeEnvelope();

    const resource = await action.run({ envelope, path: outPath });

    expect(existsSync(outPath)).toBe(true);
    expect(resource.path).toBe(outPath);
    expect(resource.envelopeCid).toBe("bafy-fake-cid-1");
    const content = readFileSync(outPath, "utf-8");
    expect(JSON.parse(content)).toEqual(envelope);
    // Pretty-printed; should contain newlines.
    expect(content.includes("\n")).toBe(true);
  });

  it("rejects relative paths", async () => {
    const action = makeWriteMementoFileAction();
    await expect(
      action.run({ envelope: makeFakeEnvelope(), path: "relative/path.json" }),
    ).rejects.toThrow(/must be absolute/);
  });

  it("Action shape: describeResource includes path + cid + sha256", () => {
    const action = makeWriteMementoFileAction();
    const desc = action.describeResource({
      path: "/tmp/m.json",
      contentSha256: "deadbeef",
      bytesWritten: 42,
      envelopeCid: "bafy-x",
    });
    expect(desc).toMatch(/wrote 42 bytes/);
    expect(desc).toMatch(/cid: bafy-x/);
    expect(desc).toMatch(/\/tmp\/m\.json/);
    expect(desc).toMatch(/sha256:deadbeef/);
  });

  it("Action shape: serializeInput uses path + envelope CID", () => {
    const action = makeWriteMementoFileAction();
    expect(
      action.serializeInput({
        path: "/abs/m.json",
        envelope: makeFakeEnvelope(),
      }),
    ).toEqual({
      path: "/abs/m.json",
      envelopeCid: "bafy-fake-cid-1",
    });
  });
});
