#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

import { createPrivateKey, sign } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, realpathSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const toolsDir = dirname(fileURLToPath(import.meta.url));
const exhibitRoot = resolve(toolsDir, "..");
const repoRoot = resolve(exhibitRoot, "../../..");
const packagesRoot = join(exhibitRoot, "packages");
const packageDirs = [
  "safe-json-1.4.1",
  "safe-json-1.4.2-lie",
  "safe-json-1.4.2-substituted",
  "safe-json-1.4.2-weakened",
].map((name) => join(packagesRoot, name));

const pathWithGoBin = `${process.env.HOME}/go/bin:${process.env.PATH ?? ""}`;

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    env: { ...process.env, PATH: pathWithGoBin },
    encoding: options.encoding ?? "utf8",
    stdio: options.stdio ?? "pipe",
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed\nstdout:\n${result.stdout ?? ""}\nstderr:\n${result.stderr ?? ""}`,
    );
  }
  return result;
}

function commandPath(name, fallbacks = []) {
  const result = spawnSync("sh", ["-c", `command -v ${name}`], {
    env: { ...process.env, PATH: pathWithGoBin },
    encoding: "utf8",
  });
  if (result.status === 0) {
    return result.stdout.trim();
  }
  for (const fallback of fallbacks) {
    if (existsSync(fallback)) {
      return fallback;
    }
  }
  throw new Error(`missing required tool: ${name}`);
}

function ensureEd25519Key(privateKey, publicKey) {
  if (!existsSync(privateKey)) {
    run("openssl", ["genpkey", "-algorithm", "Ed25519", "-out", privateKey]);
  }
  run("openssl", ["pkey", "-in", privateKey, "-pubout", "-out", publicKey]);
}

function blake3Hex(path) {
  return run("b3sum", ["--length", "64", "--no-names", path]).stdout.trim();
}

function sha256Hex(path) {
  return run("shasum", ["-a", "256", path]).stdout.trim().split(/\s+/)[0];
}

function npmPack(packageDir) {
  const temp = run("mktemp", ["-d"]).stdout.trim();
  try {
    run("npm", ["pack", "--silent", "--pack-destination", temp], { cwd: packageDir });
    const tgz = readdirSync(temp).find((entry) => entry.endsWith(".tgz"));
    if (!tgz) {
      throw new Error(`npm pack produced no tgz in ${temp}`);
    }
    run("cp", [join(temp, tgz), join(packageDir, "package.tgz")]);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

function writeSlsaVsa(packageDir, slsaDir, packageJson) {
  const privateKey = join(slsaDir, "vsa.key");
  const publicKey = join(slsaDir, "vsa.pub");
  ensureEd25519Key(privateKey, publicKey);

  const digest = blake3Hex(join(packageDir, "package.tgz"));
  const resourceUri = `pkg:npm/${packageJson.name}@${packageJson.version}`;
  const payloadObj = {
    _type: "https://in-toto.io/Statement/v1",
    predicateType: "https://slsa.dev/verification_summary/v1",
    subject: [
      {
        name: resourceUri,
        digest: {
          "blake3-512": digest,
        },
      },
    ],
    predicate: {
      timeVerified: "2026-05-08T00:00:00Z",
      verifier: {
        id: "https://provekit.dev/menagerie/supply-chain-rails/slsa-vsa-verifier/v0.1",
      },
      verificationResult: "PASSED",
      verifiedLevels: ["SLSA_BUILD_LEVEL_2"],
      resourceUri,
      policy: {
        uri: "provekit://menagerie/supply-chain-rails/policies/native-slsa-vsa",
      },
      slsaVersion: "1.0",
    },
  };
  const payloadType = "application/vnd.in-toto+json";
  const payload = Buffer.from(JSON.stringify(payloadObj));
  const pae = Buffer.concat([
    Buffer.from(`DSSEv1 ${payloadType.length} ${payloadType} ${payload.length} `),
    payload,
  ]);
  const signature = sign(null, pae, createPrivateKey(readFileSync(privateKey)));
  const envelope = {
    payloadType,
    payload: payload.toString("base64"),
    signatures: [
      {
        keyid: "",
        sig: signature.toString("base64"),
      },
    ],
  };
  writeFileSync(join(slsaDir, "vsa.jsonl"), `${JSON.stringify(envelope)}\n`);

  const slsaVerifier = commandPath("slsa-verifier", [join(process.env.HOME, "go/bin/slsa-verifier")]);
  run(slsaVerifier, [
    "verify-vsa",
    "--subject-digest",
    `blake3-512:${digest}`,
    "--attestation-path",
    join(slsaDir, "vsa.jsonl"),
    "--verifier-id",
    payloadObj.predicate.verifier.id,
    "--resource-uri",
    resourceUri,
    "--verified-level",
    "SLSA_BUILD_LEVEL_2",
    "--public-key-path",
    publicKey,
  ]);
}

function writeInToto(packageDir, inTotoDir) {
  const layoutKey = join(inTotoDir, "layout.key");
  const layoutPub = join(inTotoDir, "layout.pub");
  const functionaryKey = join(inTotoDir, "functionary.key");
  const functionaryPub = join(inTotoDir, "functionary.pub");
  ensureEd25519Key(layoutKey, layoutPub);
  ensureEd25519Key(functionaryKey, functionaryPub);

  for (const entry of readdirSync(inTotoDir)) {
    if (entry.startsWith("safe-json-pack.") && entry.endsWith(".link")) {
      rmSync(join(inTotoDir, entry));
    }
  }
  run("in-toto-run", [
    "-n",
    "safe-json-pack",
    "-m",
    "package.json",
    "index.js",
    "contracts.ts",
    "contracts.json",
    "-p",
    "package.tgz",
    "--signing-key",
    functionaryKey,
    "--metadata-directory",
    inTotoDir,
    "--no-command",
  ], { cwd: packageDir });

  const inTotoRun = realpathSync(commandPath("in-toto-run", [join(process.env.HOME, ".local/bin/in-toto-run")]));
  const inTotoPython = join(dirname(inTotoRun), "python");
  run(inTotoPython, [
    join(toolsDir, "write-in-toto-layout.py"),
    "--layout-key",
    layoutKey,
    "--functionary-pub",
    functionaryPub,
    "--output",
    join(inTotoDir, "root.layout"),
  ]);
  run("in-toto-verify", [
    "--layout",
    join(inTotoDir, "root.layout"),
    "--verification-keys",
    layoutPub,
    "--link-dir",
    inTotoDir,
  ]);
}

for (const packageDir of packageDirs) {
  const attestationsDir = join(packageDir, "attestations");
  const slsaDir = join(attestationsDir, "slsa");
  const inTotoDir = join(attestationsDir, "in-toto");
  mkdirSync(slsaDir, { recursive: true });
  mkdirSync(inTotoDir, { recursive: true });

  npmPack(packageDir);
  const packageJson = JSON.parse(readFileSync(join(packageDir, "package.json"), "utf8"));
  writeSlsaVsa(packageDir, slsaDir, packageJson);
  writeInToto(packageDir, inTotoDir);

  process.stdout.write(
    `${basename(packageDir)} package.tgz blake3-512:${blake3Hex(join(packageDir, "package.tgz"))} sha256:${sha256Hex(join(packageDir, "package.tgz"))}\n`,
  );
}
