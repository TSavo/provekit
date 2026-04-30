/**
 * End-to-end demo: lift parseInt invariants → canonicalize → sign → dump.
 *
 * The framework's first operational demonstration of the chain
 *   .invariant.ts file
 *     → liftProject (TS-IR lifter)
 *     → propertyHashFromFormula (AST canonicalizer)
 *     → ClaimEnvelope build + signEnvelope (ed25519, deterministic seed)
 *     → computeEnvelopeCid
 *     → verifyEnvelopeSignature (round-trip)
 *     → JSON dump to scripts/output/parseInt-mementos/<name>.json
 *
 * Run: npx tsx scripts/demo-end-to-end-parseint.ts
 *
 * Note on `producedAt`: this demo pins the timestamp to the unix
 * epoch so re-runs produce byte-identical mementos. That makes the
 * pipeline's determinism property visible (same input → same CID,
 * always). Production minting uses real ISO-8601 timestamps; the
 * stable-hash story still holds, the bytes just won't match between
 * runs because the timestamp varies.
 */

import path from "node:path";
import fs from "node:fs";
import { createHash } from "node:crypto";
import ts from "typescript";

import { liftProject, type LiftedProperty } from "../src/ir/lift/index.js";
import { propertyHashFromFormula } from "../src/canonicalizer/index.js";
import { generateKeypair } from "../src/producerKeys/index.js";
import {
  signEnvelope,
  computeEnvelopeCid,
  verifyEnvelopeSignature,
  VARIANT_SCHEMA_CIDS,
  type ClaimEnvelope,
} from "../src/claimEnvelope/index.js";

const PRODUCER_ID = "ts-kit-demo@0.0.1";
const SPEC_BINDING_NAMESPACE = "ECMAScript-262:parseInt";
const REPO_ROOT = path.resolve(__dirname, "..");
const FIXTURE_DIR = path.join(REPO_ROOT, "src/ir/lift/__fixtures__");
const FIXTURE_PATH = path.join(FIXTURE_DIR, "parseInt.invariant.ts");
const STUB_PATH = path.join(FIXTURE_DIR, "provekit-ir.d.ts");
const OUTPUT_DIR = path.join(REPO_ROOT, "scripts/output/parseInt-mementos");

function buildProgram(): ts.Program {
  const fileMap = new Map<string, string>();
  fileMap.set(STUB_PATH, fs.readFileSync(STUB_PATH, "utf8"));
  fileMap.set(FIXTURE_PATH, fs.readFileSync(FIXTURE_PATH, "utf8"));

  const compilerOptions: ts.CompilerOptions = {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    skipLibCheck: true,
    noEmit: true,
    esModuleInterop: true,
  };

  const host = ts.createCompilerHost(compilerOptions, true);
  const realGetSourceFile = host.getSourceFile.bind(host);
  host.getSourceFile = (fileName, languageVersion, onError, shouldCreateNewSourceFile) => {
    if (fileMap.has(fileName)) {
      return ts.createSourceFile(fileName, fileMap.get(fileName)!, languageVersion, true);
    }
    return realGetSourceFile(fileName, languageVersion, onError, shouldCreateNewSourceFile);
  };
  const realFileExists = host.fileExists.bind(host);
  host.fileExists = (fn) => fileMap.has(fn) || realFileExists(fn);
  const realReadFile = host.readFile.bind(host);
  host.readFile = (fn) => fileMap.get(fn) ?? realReadFile(fn);

  return ts.createProgram({
    rootNames: Array.from(fileMap.keys()),
    options: compilerOptions,
    host,
  });
}

function bindingHashFor(propertyName: string): string {
  const seed = `${SPEC_BINDING_NAMESPACE}:${propertyName}`;
  return createHash("sha256").update(seed, "utf8").digest("hex").slice(0, 16);
}

function buildEnvelope(property: LiftedProperty, producedAt: string): Omit<ClaimEnvelope, "cid"> {
  return {
    schemaVersion: "1",
    bindingHash: bindingHashFor(property.name),
    propertyHash: propertyHashFromFormula(property.formula),
    verdict: "holds",
    producedBy: PRODUCER_ID,
    producedAt,
    inputCids: [],
    evidence: {
      kind: "legacy-witness",
      schema: VARIANT_SCHEMA_CIDS["legacy-witness"],
      body: {
        rawWitness: JSON.stringify(property.formula),
        legacyProducerId: PRODUCER_ID,
      },
    },
  };
}

async function main(): Promise<void> {
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });

  const program = buildProgram();
  const lift = liftProject(program);

  if (lift.diagnostics.length > 0) {
    console.error("lift produced diagnostics:");
    for (const d of lift.diagnostics) console.error("  -", String(d.messageText));
    process.exitCode = 1;
    return;
  }

  const properties = lift.properties.filter((p) => p.filePath === FIXTURE_PATH);
  if (properties.length === 0) {
    console.error(`no properties lifted from ${FIXTURE_PATH}`);
    process.exitCode = 1;
    return;
  }

  const seed = Buffer.from("ts-kit-demo-seed-32-bytes-padding!").subarray(0, 32);
  const keypair = generateKeypair({ seed });
  const producedAt = new Date(0).toISOString();

  let totalBytes = 0;
  let validCount = 0;

  console.log(`Lifted ${properties.length} properties from parseInt.invariant.ts`);
  console.log(`Producer: ${PRODUCER_ID}`);
  console.log(`Producer pubkey (DER/spki, base64): ${keypair.publicKey
    .export({ format: "der", type: "spki" })
    .toString("base64")}`);
  console.log("");

  for (const property of properties) {
    const unsigned = buildEnvelope(property, producedAt);
    const signature = signEnvelope(unsigned, keypair.privateKey);
    const cid = computeEnvelopeCid(unsigned);
    const envelope: ClaimEnvelope = {
      ...unsigned,
      producerSignature: signature,
      cid,
    };

    const valid = verifyEnvelopeSignature(envelope, keypair.publicKey);
    if (valid) validCount += 1;

    const json = JSON.stringify(envelope, null, 2) + "\n";
    const outFile = path.join(OUTPUT_DIR, `${property.name}.json`);
    fs.writeFileSync(outFile, json, "utf8");
    totalBytes += Buffer.byteLength(json, "utf8");

    console.log(`  ${property.name}`);
    console.log(`    bindingHash:  ${envelope.bindingHash}`);
    console.log(`    propertyHash: ${envelope.propertyHash}`);
    console.log(`    cid:          ${envelope.cid}`);
    console.log(`    signature ok: ${valid}`);
  }

  console.log("");
  console.log(
    `Total: ${properties.length} properties signed, ${validCount}/${properties.length} signatures valid, total bytes: ${totalBytes}`,
  );

  if (validCount !== properties.length) {
    process.exitCode = 1;
  }
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
