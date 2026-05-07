import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { computeCid, SELF_IDENTIFYING_HASH_RE } from "./hash.js";

interface CicpVector {
  name: string;
  body: string;
  expectedCid?: string;
  shouldPass: boolean;
  errorContains?: string;
}

interface CicpVectorCatalog {
  vectors: CicpVector[];
}

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const vectorDir = resolve(__dirname, "../../../../protocol/conformance/cicp");
const catalog = JSON.parse(
  readFileSync(resolve(vectorDir, "vectors.json"), "utf8"),
) as CicpVectorCatalog;

function readVectorBody(vector: CicpVector): unknown {
  return JSON.parse(readFileSync(resolve(vectorDir, vector.body), "utf8"));
}

function validateClosedInputCids(body: unknown): string[] {
  if (!isRecord(body)) {
    return ["body must be a JSON object"];
  }

  const inputCids = body.inputCids;
  if (!Array.isArray(inputCids)) {
    return ["inputCids must be an array"];
  }

  const declaredInputs = new Set(inputCids.filter(isCidString));
  const missing = [...collectRequiredDependencyCids(body)]
    .filter((cid) => !declaredInputs.has(cid))
    .sort();

  return missing.map((cid) => `inputCids missing required CID ${cid}`);
}

function collectRequiredDependencyCids(value: unknown): Set<string> {
  const cids = new Set<string>();
  collectRequiredDependencyCidsInto(value, cids, undefined);
  return cids;
}

function collectRequiredDependencyCidsInto(
  value: unknown,
  cids: Set<string>,
  key: string | undefined,
): void {
  if (key === "inputCids") {
    return;
  }

  if (isCidString(value)) {
    cids.add(value);
    return;
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectRequiredDependencyCidsInto(item, cids, key);
    }
    return;
  }

  if (isRecord(value)) {
    for (const [childKey, childValue] of Object.entries(value)) {
      collectRequiredDependencyCidsInto(childValue, cids, childKey);
    }
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isCidString(value: unknown): value is string {
  return typeof value === "string" && SELF_IDENTIFYING_HASH_RE.test(value);
}

describe("CICP conformance vectors", () => {
  const passingVectors = catalog.vectors.filter((vector) => vector.shouldPass);
  const failingVectors = catalog.vectors.filter((vector) => !vector.shouldPass);

  for (const vector of passingVectors) {
    it(`${vector.name} derives the catalog-pinned BLAKE3-512 CID`, () => {
      const body = readVectorBody(vector);

      expect(validateClosedInputCids(body)).toEqual([]);
      expect(computeCid(canonicalEncode(body))).toBe(vector.expectedCid);
    });
  }

  for (const vector of failingVectors) {
    it(`${vector.name} fails closed on missing inputCids dependencies`, () => {
      const body = readVectorBody(vector);
      const errors = validateClosedInputCids(body);

      expect(errors.join("\n")).toContain(vector.errorContains);
    });
  }
});
