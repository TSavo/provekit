#!/usr/bin/env node
/*
 * emit_smt.js — consume the AST-walked facts (walker.json) and emit SMT-LIB2.
 *
 * LAW: this script reads ONLY the JSON produced by Base64Walker. It never reads
 * the Java source, and it hard-codes no base64 knowledge: the table bytes, the
 * mask, the pad, the per-char shift ops/amounts, and the block structure all come
 * from the walked tree. The ONLY thing fixed here is the structural model of how
 * `ibitWorkArea` accumulates input bytes, which the walker also recorded as the
 * statement `ibitWorkArea = (ibitWorkArea << 8) + b` (see report; modeled below).
 *
 * Emits four files for a chosen vendor input (default "foo" -> "Zm9v"):
 *   strong_derive.smt2   : x pinned to vendor input, STRONG constraints, derive Y.
 *   strong_unique.smt2   : x pinned, STRONG constraints, assert Y != vendor -> UNSAT.
 *   refute_alphabet.smt2 : STRONG constraints, assert some Y byte is '-' or '_' -> UNSAT.
 *   weak_alphabet.smt2   : WEAK constraints (membership+length only), claim an
 *                          out-of-alphabet output byte -> UNSAT (generalized refutation).
 */

const fs = require("fs");

const factsPath = process.argv[2];
const outDir = process.argv[3] || ".";
const facts = JSON.parse(fs.readFileSync(factsPath, "utf8"));

// ---- the vendor vector (real assertion, Base64Test.java:878) -------------
// assertEquals("Zm9v", Base64.encodeBase64String(StringUtils.getBytesUtf8("foo")));
const VENDOR_INPUT = "foo";                       // UTF-8 bytes
const VENDOR_OUTPUT = "Zm9v";                     // sworn output
const inputBytes = Array.from(Buffer.from(VENDOR_INPUT, "utf8"));   // [102,111,111]
const outputBytes = Array.from(Buffer.from(VENDOR_OUTPUT, "ascii"));

const table = facts.table.bytes;                  // 64 entries, walked
const MASK = Number(facts.constants.MASK_6BITS.value);   // 63
const PAD = facts.pad.value;                       // 61

// table-as-array SMT: (declare table : (Array (_ BitVec 6) (_ BitVec 8)))
// We build it from the walked literal via nested stores; soundness rests on the
// walked `static final` modifier (table is immutable -> the array is a constant).
function tableArrayDecl() {
  let expr = "((as const (Array (_ BitVec 6) (_ BitVec 8))) #x00)";
  for (let i = 0; i < table.length; i++) {
    const idx = "(_ bv" + i + " 6)";
    const val = "(_ bv" + table[i] + " 8)";
    expr = "(store " + expr + " " + idx + " " + val + ")";
  }
  return expr;
}

// Emit the index expression for one walked char-emit record, given the 24-bit
// work-area bitvector name `w`. Reproduces `w OP amount & MASK` as a 6-bit value.
// op/amount come straight from the walked AST node.
function indexExpr6(rec, wName, wWidth) {
  // compute (w OP amount) then & MASK, then extract low 6 bits.
  let shifted;
  if (rec.op === "SHR") {
    shifted = "(bvlshr " + wName + " (_ bv" + rec.amount + " " + wWidth + "))";
  } else if (rec.op === "SHL") {
    shifted = "(bvshl " + wName + " (_ bv" + rec.amount + " " + wWidth + "))";
  } else { // ID — bare & MASK
    shifted = wName;
  }
  const masked = "(bvand " + shifted + " (_ bv" + MASK + " " + wWidth + "))";
  // low 6 bits as a (_ BitVec 6)
  return "((_ extract 5 0) " + masked + ")";
}

const header = (title) =>
  "; " + title + "\n" +
  "; AUTO-EMITTED from AST-walked facts (menagerie/base64-generalization/walker.json).\n" +
  "; Table, mask, pad, shift ops/amounts all derived from the commons-codec tree.\n" +
  "(set-logic QF_ABV)\n";

const tableDecl =
  "(define-fun T () (Array (_ BitVec 6) (_ BitVec 8)) " + tableArrayDecl() + ")\n";

// ---- STRONG model for a 3-byte full block --------------------------------
// Walked accumulation: ibitWorkArea = (ibitWorkArea<<8)+b, three times.
// => w24 = (b0<<16) | (b1<<8) | b2. The four chars come from block3 records.
function strongBlock(declInputs, pinInputs) {
  if (facts.block3.length !== 4) throw new Error("expected 4 block3 chars");
  let s = "";
  // inputs as BV8
  s += "(declare-const b0 (_ BitVec 8))\n";
  s += "(declare-const b1 (_ BitVec 8))\n";
  s += "(declare-const b2 (_ BitVec 8))\n";
  // 24-bit work area, structural accumulation (zero-extend then shift/or)
  s += "(define-fun w () (_ BitVec 24) (bvor (bvor " +
       "(bvshl ((_ zero_extend 16) b0) (_ bv16 24)) " +
       "(bvshl ((_ zero_extend 16) b1) (_ bv8 24))) " +
       "((_ zero_extend 16) b2)))\n";
  // four output chars
  s += "(declare-const y0 (_ BitVec 8))\n(declare-const y1 (_ BitVec 8))\n";
  s += "(declare-const y2 (_ BitVec 8))\n(declare-const y3 (_ BitVec 8))\n";
  const ys = ["y0", "y1", "y2", "y3"];
  for (let i = 0; i < 4; i++) {
    s += "(assert (= " + ys[i] + " (select T " + indexExpr6(facts.block3[i], "w", 24) + ")))\n";
  }
  if (pinInputs) {
    s += "(assert (= b0 (_ bv" + inputBytes[0] + " 8)))\n";
    s += "(assert (= b1 (_ bv" + inputBytes[1] + " 8)))\n";
    s += "(assert (= b2 (_ bv" + inputBytes[2] + " 8)))\n";
  }
  return s;
}

// strong_derive: pin input, derive Y, check-sat + get-model.
function emitStrongDerive() {
  let s = header("STRONG: derive Y for x=\"foo\" (vendor vector Base64Test.java:878)");
  s += tableDecl;
  s += strongBlock(true, true);
  s += "(check-sat)\n(get-value (y0 y1 y2 y3))\n";
  return s;
}

// strong_unique: pin input + assert Y != vendor output -> UNSAT.
function emitStrongUnique() {
  let s = header("STRONG uniqueness: x=\"foo\" pinned, Y != \"Zm9v\" must be UNSAT");
  s += tableDecl;
  s += strongBlock(true, true);
  // assert the disjunction: at least one output char differs from vendor.
  const diffs = outputBytes.map((c, i) => "(not (= y" + i + " (_ bv" + c + " 8)))");
  s += "(assert (or " + diffs.join(" ") + "))\n";
  s += "(check-sat)\n";
  return s;
}

// refute_alphabet: STRONG constraints, free input, assert one emitted char is
// outside the standard table (e.g. the url-safe '-' 0x2d or '_' 0x5f) -> UNSAT.
// This is the url-safe confusion refutation, WITHOUT a vector collision.
function emitRefuteAlphabet() {
  let s = header("REFUTATION: under STRONG (standard table), no output char can be '-' or '_'");
  s += tableDecl;
  s += strongBlock(true, false); // free input (any 3-byte block)
  // '-' = 45, '_' = 95 (url-safe-only chars). Claim some output char equals one.
  s += "(assert (or " +
       "(= y0 (_ bv45 8)) (= y1 (_ bv45 8)) (= y2 (_ bv45 8)) (= y3 (_ bv45 8)) " +
       "(= y0 (_ bv95 8)) (= y1 (_ bv95 8)) (= y2 (_ bv95 8)) (= y3 (_ bv95 8))))\n";
  s += "(check-sat)\n";
  return s;
}

// weak_alphabet: the WEAK generalization. Membership: every output char is a
// member of the walked table OR the pad. Then claim an output char is out of
// alphabet (e.g. '-') -> UNSAT. No bit equations; pure membership.
function emitWeakAlphabet() {
  let s = header("WEAK: every Y byte in (table union {pad}); claim a '-' byte -> UNSAT");
  // membership predicate built from the walked table literal.
  s += "(define-fun inAlphabet ((c (_ BitVec 8))) Bool (or ";
  const members = table.map((b) => "(= c (_ bv" + b + " 8))");
  members.push("(= c (_ bv" + PAD + " 8))");
  s += members.join(" ") + "))\n";
  s += "(declare-const yk (_ BitVec 8))\n";
  s += "(assert (inAlphabet yk))\n";       // the weak contract on this byte
  s += "(assert (= yk (_ bv45 8)))\n";      // consumer claims it is '-' (url-safe)
  s += "(check-sat)\n";
  return s;
}

fs.writeFileSync(outDir + "/strong_derive.smt2", emitStrongDerive());
fs.writeFileSync(outDir + "/strong_unique.smt2", emitStrongUnique());
fs.writeFileSync(outDir + "/refute_alphabet.smt2", emitRefuteAlphabet());
fs.writeFileSync(outDir + "/weak_alphabet.smt2", emitWeakAlphabet());

// Echo the expected vendor mapping for run.sh to assert against.
const expected = outputBytes.map((c) => "(_ bv" + c + " 8)").join(" ");
console.log("VENDOR_INPUT=" + VENDOR_INPUT);
console.log("VENDOR_OUTPUT=" + VENDOR_OUTPUT);
console.log("EXPECTED_Y=" + outputBytes.join(","));
console.log("EXPECTED_CHARS=" + VENDOR_OUTPUT.split("").join(","));
