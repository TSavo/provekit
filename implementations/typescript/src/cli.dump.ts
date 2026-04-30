/**
 * `provekit dump` — inspect a binary `.proof` file.
 *
 * A `.proof` file is deterministic CBOR (per protocol/specs/2026-04-30-proof-file-format.md).
 * Without an inspection tool, the format is hostile. This command:
 *
 *   1. Reads the .proof file's bytes
 *   2. Recomputes the trust-root CID; verifies it matches the filename
 *   3. Decodes the CBOR catalog envelope
 *   4. Recomputes each member's CID; reports any mismatches
 *   5. Pretty-prints the catalog + decoded member envelopes as JSON
 *
 * Usage:
 *   provekit dump <file>.proof [--no-members] [--json]
 *
 * Flags:
 *   --no-members   Skip pretty-printing member envelope bodies (just headers + CIDs)
 *   --json         Emit a single JSON document instead of human-readable formatting
 *
 * Exit codes:
 *   0 — file decoded successfully and all CIDs match
 *   1 — decode failed or any CID mismatch (fail-closed; matches spec rules 1-2)
 */

import { readFileSync } from "node:fs";
import { resolve, basename } from "node:path";
import { decodeProofEnvelope } from "./proofEnvelope/index.js";
import { computeEnvelopeCid } from "./claimEnvelope/cid.js";
import { computeCid } from "./canonicalizer/hash.js";
import type { ClaimEnvelope } from "./claimEnvelope/types.js";

interface DumpFlags {
  showMembers: boolean;
  jsonOutput: boolean;
}

function parseDumpFlags(argv: string[]): { filePath: string | null; flags: DumpFlags } {
  let filePath: string | null = null;
  const flags: DumpFlags = { showMembers: true, jsonOutput: false };
  for (const a of argv) {
    if (a === "--no-members") flags.showMembers = false;
    else if (a === "--json") flags.jsonOutput = true;
    else if (a === "-h" || a === "--help") {
      printDumpHelp();
      process.exit(0);
    } else if (!a.startsWith("--") && filePath === null) {
      filePath = a;
    }
  }
  return { filePath, flags };
}

function printDumpHelp(): void {
  process.stderr.write(`provekit dump — inspect a .proof file

Usage:
  provekit dump <file>.proof [--no-members] [--json]

Verifies the file's bytes hash to its filename CID (trust root) and
each embedded member's CID matches its envelope identity, then prints
the decoded catalog and members for inspection.

Exit 1 on any decode failure or CID mismatch.
`);
}

export async function runDump(argv: string[]): Promise<void> {
  const { filePath, flags } = parseDumpFlags(argv);
  if (!filePath) {
    process.stderr.write("error: 'provekit dump' requires <file>.proof\n");
    printDumpHelp();
    process.exit(1);
  }

  const absPath = resolve(filePath);
  const filename = basename(absPath);
  // Self-identifying CID filenames: "<algorithm>-<bits>:<hex>.proof".
  // v1.1.0 uses "blake3-512:<128-hex>.proof".
  const m = filename.match(/^([a-z0-9]+-[0-9]+:[0-9a-f]+)\.proof$/);
  const filenameCid = m ? m[1]! : null;

  let bytes: Buffer;
  try {
    bytes = readFileSync(absPath);
  } catch (e) {
    process.stderr.write(`error: cannot read ${absPath}: ${(e as Error).message}\n`);
    process.exit(1);
  }

  const derivedCid = computeCid(bytes);

  const errors: string[] = [];

  // Rule 1: filename CID matches content hash.
  let trustRootOk = true;
  if (filenameCid === null) {
    errors.push(
      `filename "${filename}" does not match <cid>.proof pattern; cannot verify trust root from filename`,
    );
    trustRootOk = false;
  } else if (filenameCid !== derivedCid) {
    errors.push(
      `rule 1 (trust root): filename CID ${filenameCid} != content hash ${derivedCid}`,
    );
    trustRootOk = false;
  }

  // Decode (even on filename mismatch, so user can still see structure).
  let catalog;
  try {
    catalog = decodeProofEnvelope(new Uint8Array(bytes));
  } catch (e) {
    errors.push(`decode: ${(e as Error).message}`);
    if (flags.jsonOutput) {
      process.stdout.write(
        JSON.stringify(
          { ok: false, errors, derivedCid, filenameCid, fileSize: bytes.length },
          null,
          2,
        ) + "\n",
      );
    } else {
      printErrors(errors);
      process.stderr.write(`file size: ${bytes.length} bytes\n`);
      process.stderr.write(`derived CID: ${derivedCid}\n`);
      if (filenameCid) process.stderr.write(`filename CID: ${filenameCid}\n`);
    }
    process.exit(1);
  }

  // Rule 2: each member's CID matches its envelope identity.
  interface MemberSummary {
    cid: string;
    derivedCid: string;
    cidMatch: boolean;
    envelope: ClaimEnvelope | null;
    parseError?: string;
  }
  const memberSummaries: MemberSummary[] = [];
  for (const [cid, memberBytes] of catalog.members) {
    let env: ClaimEnvelope | null = null;
    let derived = "";
    let parseError: string | undefined;
    try {
      env = JSON.parse(Buffer.from(memberBytes).toString("utf8")) as ClaimEnvelope;
      derived = computeEnvelopeCid(env);
      if (derived !== cid) {
        errors.push(`rule 2 (member ${cid.slice(0, 12)}…): bytes derive to ${derived}`);
      }
    } catch (e) {
      parseError = (e as Error).message;
      errors.push(`member ${cid.slice(0, 12)}…: failed to parse envelope: ${parseError}`);
    }
    memberSummaries.push({
      cid,
      derivedCid: derived,
      cidMatch: derived === cid,
      envelope: env,
      ...(parseError ? { parseError } : {}),
    });
  }

  const ok = errors.length === 0;

  if (flags.jsonOutput) {
    const out: Record<string, unknown> = {
      ok,
      errors,
      filename,
      filenameCid,
      derivedCid,
      trustRootOk,
      fileSize: bytes.length,
      catalog: {
        kind: catalog.kind,
        name: catalog.name,
        version: catalog.version,
        signer: catalog.signer,
        signatureBytes: catalog.signature.length,
        declaredAt: catalog.declaredAt,
        memberCount: catalog.members.size,
      },
      members: memberSummaries.map((m) => {
        const out: Record<string, unknown> = {
          cid: m.cid,
          derivedCid: m.derivedCid,
          cidMatch: m.cidMatch,
        };
        if (m.parseError) out.parseError = m.parseError;
        if (flags.showMembers && m.envelope) out.envelope = m.envelope;
        return out;
      }),
    };
    if (catalog.dependsOn) (out.catalog as Record<string, unknown>).dependsOn = catalog.dependsOn;
    process.stdout.write(JSON.stringify(out, null, 2) + "\n");
  } else {
    process.stdout.write(`# ${filename}\n`);
    process.stdout.write(`file size:    ${bytes.length} bytes\n`);
    process.stdout.write(`derived CID:  ${derivedCid}${trustRootOk ? "  ✓" : "  ✗"}\n`);
    if (filenameCid) {
      process.stdout.write(`filename CID: ${filenameCid}\n`);
    }
    process.stdout.write(`\n## catalog\n`);
    process.stdout.write(`  kind:        ${catalog.kind}\n`);
    process.stdout.write(`  name:        ${catalog.name}\n`);
    process.stdout.write(`  version:     ${catalog.version}\n`);
    process.stdout.write(`  signer:      ${catalog.signer}\n`);
    process.stdout.write(`  signature:   ${catalog.signature.length} bytes\n`);
    process.stdout.write(`  declaredAt:  ${catalog.declaredAt}\n`);
    if (catalog.dependsOn) {
      process.stdout.write(`  dependsOn:   ${JSON.stringify(catalog.dependsOn)}\n`);
    }
    process.stdout.write(`\n## members (${catalog.members.size})\n`);
    for (const ms of memberSummaries) {
      const marker = ms.cidMatch ? "✓" : "✗";
      process.stdout.write(`  ${marker} ${ms.cid}\n`);
      if (!ms.cidMatch && ms.derivedCid) {
        process.stdout.write(`    (bytes derive to ${ms.derivedCid})\n`);
      }
      if (ms.parseError) {
        process.stdout.write(`    parse error: ${ms.parseError}\n`);
      }
      if (flags.showMembers && ms.envelope) {
        const lines = JSON.stringify(ms.envelope, null, 2).split("\n");
        for (const ln of lines) process.stdout.write(`      ${ln}\n`);
      }
    }
    if (errors.length > 0) {
      process.stdout.write(`\n## errors (${errors.length})\n`);
      for (const e of errors) process.stdout.write(`  ✗ ${e}\n`);
    }
  }

  process.exit(ok ? 0 : 1);
}

function printErrors(errors: string[]): void {
  for (const e of errors) process.stderr.write(`  ✗ ${e}\n`);
}
