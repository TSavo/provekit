/**
 * Cross-implementation golden tests against fixtures.toml.
 *
 * Loads the canonical conformance fixtures and asserts the TS kit produces
 * the pinned protocol CID for every covered fixture: formula-level AND
 * declaration-level (contract / bridge declarations).
 *
 * Spec: conformance/fixtures.toml -- catalog-pinned BLAKE3-512 CIDs.
 *
 * To regenerate Rust golden (do NOT touch in this PR):
 *   cd tools/v1-3-fields-probe && cargo run
 */

import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalEncode } from "../claimEnvelope/canonicalize.js";
import { computeCid } from "./hash.js";
import type { IrFormula, IrTerm, Sort } from "./irFormula.js";

// -----------------------------------------------------------------------
// Minimal TOML parser for fixtures.toml
// -----------------------------------------------------------------------

interface Fixture {
  name: string;
  description: string;
  jcs: string;
  hash: string;
}

function parseFixturesToml(content: string): Fixture[] {
  const fixtures: Fixture[] = [];
  let current: Partial<Fixture> = {};
  let inJcs = false;
  let jcsBuffer = "";

  for (const rawLine of content.split("\n")) {
    const line = rawLine.trim();

    // [[fixture]] starts a new fixture block
    if (line.startsWith("[[fixture]]")) {
      if (current.name && current.jcs && current.hash) {
        fixtures.push(current as Fixture);
      }
      current = {};
      continue;
    }

    // Skip comments and blank lines
    if (line === "" || line.startsWith("#")) {
      continue;
    }

    // Handle multi-line jcs value (single-quoted TOML literal string)
    if (inJcs) {
      if (line.endsWith("'")) {
        jcsBuffer += line.slice(0, -1);
        current.jcs = jcsBuffer;
        inJcs = false;
        jcsBuffer = "";
      } else {
        jcsBuffer += line;
      }
      continue;
    }

    const eqIdx = line.indexOf("=");
    if (eqIdx === -1) continue;

    const key = line.slice(0, eqIdx).trim();
    let value = line.slice(eqIdx + 1).trim();

    // Remove optional trailing comment after value
    const commentIdx = value.indexOf("#");
    if (commentIdx !== -1) {
      value = value.slice(0, commentIdx).trim();
    }

    // Handle quoted values
    if (value.startsWith("'")) {
      // Single-quoted literal string -- may span multiple lines
      value = value.slice(1);
      if (value.endsWith("'")) {
        value = value.slice(0, -1);
      } else {
        // Multi-line literal string
        inJcs = true;
        jcsBuffer = value;
        continue;
      }
    } else if (value.startsWith('"')) {
      value = value.slice(1, -1);
    }

    if (key === "name") current.name = value;
    else if (key === "description") current.description = value;
    else if (key === "jcs") current.jcs = value;
    else if (key === "hash") current.hash = value;
  }

  // Push last fixture
  if (current.name && current.jcs && current.hash) {
    fixtures.push(current as Fixture);
  }

  return fixtures;
}

// -----------------------------------------------------------------------
// Load fixtures from disk
// -----------------------------------------------------------------------

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const fixturesPath = resolve(__dirname, "../../../../conformance/fixtures.toml");
const allFixtures = parseFixturesToml(readFileSync(fixturesPath, "utf8"));

// -----------------------------------------------------------------------
// Shared sort / term builders (minimal, same style as equivalence.test.ts)
// -----------------------------------------------------------------------

const Int: Sort = { kind: "primitive", name: "Int" };
const String: Sort = { kind: "primitive", name: "String" };

function constTerm(value: unknown, sort: Sort): IrTerm {
  return { kind: "const", value, sort };
}

function varTerm(name: string): IrTerm {
  return { kind: "var", name };
}

function ctorTerm(name: string, args: IrTerm[]): IrTerm {
  return { kind: "ctor", name, args };
}

// -----------------------------------------------------------------------
// Fixture IR constructors -- one per formula-level fixture
// -----------------------------------------------------------------------

function buildFormulaFor(name: string): IrFormula | null {
  switch (name) {
    case "eq_atomic":
      return {
        kind: "atomic",
        name: "=",
        args: [
          ctorTerm("parse_int", [constTerm("42", String)]),
          constTerm(42, Int),
        ],
      };

    case "pattern1_bounded_loop": {
      const x = varTerm("x");
      const zero = constTerm(0, Int);
      const hundred = constTerm(100, Int);

      const lower: IrFormula = {
        kind: "atomic",
        name: "≥",
        args: [x, zero],
      };
      const upper: IrFormula = {
        kind: "atomic",
        name: "<",
        args: [x, hundred],
      };
      const ant: IrFormula = {
        kind: "and",
        operands: [lower, upper],
      };
      const inner: IrFormula = {
        kind: "atomic",
        name: "≥",
        args: [x, zero],
      };

      return {
        kind: "forall",
        name: "x",
        sort: Int,
        body: {
          kind: "implies",
          operands: [ant, inner],
        },
      };
    }

    default:
      return null;
  }
}

// -----------------------------------------------------------------------
// Declaration-level fixture builders -- contract_decl, bridge_decl.
//
// The JCS shapes differ from formula fixtures:
//   - contract_decl emits an ARRAY of declaration objects (matching
//     Rust's `marshal_declarations` and Ruby's `Provekit::IR.marshal_declarations`).
//   - bridge_decl emits a single declaration OBJECT.
//
// Both shapes flow through the same generic `canonicalEncode` (sorted-keys
// JCS) because there is no separate IR type for declarations on the TS
// canonicalizer surface; conformance is judged by the resulting protocol CID.
// -----------------------------------------------------------------------

function buildDeclarationsFor(name: string): unknown | null {
  switch (name) {
    case "contract_decl": {
      const x = varTerm("x");
      const zero = constTerm(0, Int);
      const pre: IrFormula = {
        kind: "atomic",
        name: "≥",
        args: [x, zero],
      };
      // Single-element array of contract decls (matches Rust / Ruby
      // `marshal_declarations` shape).
      return [
        {
          kind: "contract",
          name: "parseInt",
          outBinding: "out",
          pre,
        },
      ];
    }

    case "bridge_decl_v1_1": {
      // v1.1 flat 9-field bridge-decl (historical bytes per
      // substrate-layers spec §4). Bare object (not array-wrapped),
      // with optional `notes` present. Field order is irrelevant
      // because canonicalEncode sorts keys lexicographically before
      // emitting.
      return {
        kind: "bridge",
        name: "myBridge",
        sourceSymbol: "source",
        sourceLayer: "c-kit",
        sourceContractCid: "bafySource",
        targetContractCid: "bafyTarget",
        targetProofCid: "bafyProof",
        targetLayer: "coq",
        notes: "some notes",
      };
    }

    default:
      return null;
  }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------

const formulaFixtures = allFixtures.filter((f) => buildFormulaFor(f.name) !== null);
const declarationFixtures = allFixtures.filter(
  (f) => buildDeclarationsFor(f.name) !== null,
);

// v1.4 BridgeDeclaration fixtures (suffix `_v1_4`) are intentionally not
// yet covered by the TypeScript canonicalizer. PR-1 (issue #219) lands
// the v1.4 canonical reference in Rust + the CDDL grammar; per-kit
// adoption (TS, java, ruby, csharp, cpp) follows in #188 / #190 / #192
// / #193 / sibling-PRs. Skipping here keeps the suite green during the
// migration window.
const DEFERRED_V14_SUFFIX = "_v1_4";
const uncoveredFixtures = allFixtures.filter(
  (f) =>
    buildFormulaFor(f.name) === null &&
    buildDeclarationsFor(f.name) === null &&
    !f.name.endsWith(DEFERRED_V14_SUFFIX),
);

describe("cross-impl golden: TS IR fixture CIDs", () => {
  if (uncoveredFixtures.length > 0) {
    it(`uncovered fixtures: ${uncoveredFixtures.map((f) => f.name).join(", ")}`, () => {
      throw new Error(
        `These fixtures have no TS builder: ${uncoveredFixtures.map((f) => f.name).join(", ")}`,
      );
    });
  }

  for (const fixture of formulaFixtures) {
    it(`"${fixture.name}" — BLAKE3-512 CID matches catalog pin`, () => {
      const formula = buildFormulaFor(fixture.name)!;
      const bytes = canonicalEncode(formula);
      const actualHash = computeCid(bytes);

      expect(actualHash, `hash mismatch for "${fixture.name}"`).toBe(
        fixture.hash,
      );
    });
  }

  for (const fixture of declarationFixtures) {
    it(`"${fixture.name}" — declaration BLAKE3-512 CID matches catalog pin`, () => {
      const value = buildDeclarationsFor(fixture.name)!;
      const bytes = canonicalEncode(value);
      const actualHash = computeCid(bytes);

      expect(actualHash, `hash mismatch for "${fixture.name}"`).toBe(
        fixture.hash,
      );
    });
  }
});
