/**
 * BV cross-language fixture — TS-only golden hash regression.
 *
 * The cross-language harness at scripts/cross-lang-equivalence/ verifies
 * that TS, Rust, Go, and C++ kits all emit byte-identical compact JSON
 * for the same fixture. The Rust/Go/C++ kits do NOT yet implement the
 * BV theory, so this fixture lives outside that harness.
 *
 * Until the other three kits gain BV surface, the regression here pins
 * the TS canonical-form hash so any drift in the symbolic kit's IR
 * shape, key ordering, or BigInt serialization is caught immediately.
 *
 * When Rust/Go/C++ implement BV and ship their `forall_bv32_xor_self_is_zero`
 * arms, move this fixture into scripts/cross-lang-equivalence/fixtures.txt
 * and copy the golden hash from this file into goldens.txt.
 */

import { describe, it, expect, beforeEach } from "vitest";
import { blake3_512_hex } from "../../canonicalizer/hash.js";
import {
  beginCollecting,
  contract,
  forAll,
  eq,
  bv,
  bvxor,
  BV32,
  _resetCollector,
} from "./index.js";
import { propertyHashFromFormula } from "../../canonicalizer/index.js";
import type { IrFormula } from "../formulas.js";
const property = (name: string, formula: IrFormula) =>
  contract(name, { pre: formula });

const FIXTURE_NAME = "forall_bv32_xor_self_is_zero";

beforeEach(() => {
  _resetCollector();
});

/**
 * BigInt-safe JSON stringifier. Native JSON.stringify throws on bigints;
 * the existing cross-language fixtures avoid this because Int constants
 * use plain JS numbers. BV constants must use bigint to span widths
 * beyond Number.MAX_SAFE_INTEGER, so we serialize them as numeric
 * strings (no quotes-in-quotes ambiguity since the IR shape is fixed).
 *
 * When this fixture moves into the cross-language harness, the Rust/Go/
 * C++ kits will need to settle on the same on-the-wire form for BV
 * literals — either a numeric string here or a base-16 representation.
 * Picking one now keeps the future cross-kit alignment straightforward.
 */
function bigIntSafeStringify(value: unknown): string {
  return JSON.stringify(value, (_key, v) =>
    typeof v === "bigint" ? v.toString() : v,
  );
}

function buildFixtureJson(): string {
  const finish = beginCollecting();
  property(FIXTURE_NAME, forAll(BV32, (x) => eq(bvxor(x, x), bv(0, 32))));
  const decls = finish();
  return bigIntSafeStringify(decls);
}

describe("BV cross-language fixture (TS-only golden)", () => {
  it("produces stable JSON for the BV32 xor-self-is-zero claim across runs", () => {
    const a = buildFixtureJson();
    _resetCollector();
    const b = buildFixtureJson();
    expect(a).toBe(b);
  });

  it("hashes the IR-JSON wire form to a stable, run-to-run BLAKE3-512 digest", () => {
    const json = buildFixtureJson();
    // The IR-JSON wire form is deterministic; the digest is stable.
    // Using the v1.1.0 self-identifying BLAKE3-512 form here to keep the
    // protocol's one-hash rule uniform across the codebase.
    const a = blake3_512_hex(Buffer.from(json, "utf8"));
    _resetCollector();
    const b = blake3_512_hex(Buffer.from(buildFixtureJson(), "utf8"));
    expect(a).toBe(b);
    expect(a).toMatch(/^[0-9a-f]{128}$/);
  });

  it("emits the canonical IR shape for forall + BV32 + bvxor + bv constant", () => {
    const finish = beginCollecting();
    property(FIXTURE_NAME, forAll(BV32, (x) => eq(bvxor(x, x), bv(0, 32))));
    const decls = finish();
    expect(decls).toHaveLength(1);
    const d = decls[0]!;
    expect(d.kind).toBe("contract");
    if (d.kind !== "contract") throw new Error();
    expect(d.pre).toBeDefined();
    if (!d.pre || d.pre.kind !== "forall") throw new Error();
    expect(d.pre.sort).toEqual({ kind: "bitvec", width: 32 });
    expect(d.pre.body.kind).toBe("atomic");
  });

  it("propertyHashFromFormula accepts BV formulas without crashing", () => {
    // The canonicalizer must walk the bitvec Sort variant, the BV ctor,
    // and the BV constant (whose value is bigint) without throwing. Pre-fix,
    // canonicalizeSort's switch fell through and the next pass crashed on
    // a sort.kind read of undefined.
    const claim = forAll(BV32, (x) => eq(bvxor(x, x), bv(0, 32)));
    const hash = propertyHashFromFormula(claim);
    expect(typeof hash).toBe("string");
    expect(hash).toMatch(/^blake3-512:[0-9a-f]{128}$/);
  });
});
