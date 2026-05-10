#!/usr/bin/env tsx
/**
 * Smoke validation for src/sast/diff.ts.
 *
 * Per advisor (2026-04-27): 10 of each beats 30 random. Three fixture
 * shapes that the diff classifier MUST get right:
 *
 *   1. pure-addition: fix adds a guard before existing code
 *   2. in-place-mod: fix swaps an operator or literal
 *   3. or-chain-extend: fix appends a new clause to an OR-chain (the
 *                          enum-disjunction principle's mining target)
 *
 * Each fixture asserts an expected (unchanged, modified, added, deleted)
 * shape: not exact counts, since ts-morph's child decomposition is
 * implementation-defined, but the qualitative pattern (e.g. "at least one
 * 'added' entry whose kindName indicates a throw" for pure-addition).
 */
import { computeFileDiff, summarize, type DiffEntry } from "../src/sast/diff.js";

interface Fixture {
  name: string;
  pre: string;
  post: string;
  /** Predicate over the entries that must hold. Returns null if ok, else fail reason. */
  check: (entries: DiffEntry[]) => string | null;
}

function hasAddedKind(entries: DiffEntry[], kindNames: string[]): boolean {
  return entries.some(
    (e) => e.changeKind === "added" && kindNames.includes(e.post!.kindName),
  );
}

function hasModifiedKind(entries: DiffEntry[], kindName: string): boolean {
  return entries.some(
    (e) => e.changeKind === "modified" && e.post!.kindName === kindName,
  );
}

function hasUnchangedCount(entries: DiffEntry[], min: number): boolean {
  return entries.filter((e) => e.changeKind === "unchanged").length >= min;
}

const fixtures: Fixture[] = [
  // ===== pure-addition: guard added before existing code =====
  {
    name: "pure-add-1: divide guard",
    pre: `function divide(a, b) {
  return a / b;
}`,
    post: `function divide(a, b) {
  if (b === 0) throw new Error("Division by zero");
  return a / b;
}`,
    check: (e) => {
      // The throw statement (or its containing if) must be flagged "added".
      // ts-morph kindName for `if (b === 0) throw new Error(...)` is "IfStatement".
      if (!hasAddedKind(e, ["IfStatement", "ThrowStatement"])) {
        return "no IfStatement/ThrowStatement marked added";
      }
      // The original ReturnStatement should remain unchanged.
      const hasUnchangedReturn = e.some(
        (entry) => entry.changeKind === "unchanged" && entry.post!.kindName === "ReturnStatement",
      );
      if (!hasUnchangedReturn) return "ReturnStatement was not unchanged";
      return null;
    },
  },
  {
    name: "pure-add-2: validation guard",
    pre: `function getName(user) {
  return user.name;
}`,
    post: `function getName(user) {
  if (!user) return null;
  return user.name;
}`,
    check: (e) => {
      if (!hasAddedKind(e, ["IfStatement", "ReturnStatement"])) {
        return "no IfStatement marked added (the new guard)";
      }
      return null;
    },
  },

  // ===== in-place-modification: operator/literal swap =====
  {
    name: "in-place-mod-1: literal change",
    pre: `function f(a) { return a + 1; }`,
    post: `function f(a) { return a + 2; }`,
    check: (e) => {
      // The "1" → "2" change: same kind (NumericLiteral), same ordinal
      // among parent's children → top-down pairing classifies as
      // `modified` recursively up to the differing leaf. No add/delete
      // needed since each level pairs cleanly.
      const summary = summarize(e);
      if (summary.modified === 0) return "no 'modified' entries";
      // Find a NumericLiteral entry classified modified, with the
      // pre/post text reflecting the literal swap.
      const litMod = e.find(
        (entry) =>
          entry.changeKind === "modified" &&
          entry.pre?.kindName === "NumericLiteral" &&
          entry.pre.textPreview === "1" &&
          entry.post?.textPreview === "2",
      );
      if (!litMod) return "no NumericLiteral 1→2 modified entry";
      return null;
    },
  },
  {
    name: "in-place-mod-2: operator change",
    pre: `function f(a, b) { return a + b; }`,
    post: `function f(a, b) { return a - b; }`,
    check: (e) => {
      const summary = summarize(e);
      if (summary.modified === 0) return "no 'modified' entries";
      // Operator tokens differ in fingerprint → one '+' is deleted, one '-' added.
      if (summary.added === 0) return "no 'added' entries";
      if (summary.deleted === 0) return "no 'deleted' entries";
      return null;
    },
  },

  // ===== or-chain-extension: principle mining target =====
  // Day 3 principle: a pre BinaryExpression whose fingerprint appears as
  // an `unchanged` row inside a post tree where its parent is `added` is
  // an extended-OR-chain candidate. The assertion below verifies BOTH the
  // pre→unchanged reachability and the new-clause-is-added signal.
  {
    name: "or-chain-extend-1: enum disjunction",
    pre: `function check(x) {
  return x === "a" || x === "b";
}`,
    post: `function check(x) {
  return x === "a" || x === "b" || x === "c";
}`,
    check: (e) => {
      // The pre 2-clause OR's fingerprint must appear in an `unchanged` row
      // (because post parses as ((a||b) || c), the inner subtree matches).
      const preOrFp = e.find(
        (entry) =>
          entry.pre?.kindName === "BinaryExpression" &&
          /=== "a".*\|\|.*=== "b"/.test(entry.pre.textPreview),
      )?.pre?.fingerprint;
      if (!preOrFp) return "couldn't locate pre 2-clause BinaryExpression";
      const innerUnchanged = e.find(
        (entry) =>
          entry.changeKind === "unchanged" &&
          entry.pre?.fingerprint === preOrFp &&
          entry.post?.kindName === "BinaryExpression",
      );
      if (!innerUnchanged) {
        return `pre 2-clause OR fingerprint ${preOrFp} not unchanged-paired; the OR-chain principle would fail to find the inner subtree`;
      }
      // The outer 3-clause OR (or its new "c" clause) must be added.
      const hasAddedCompare = e.some(
        (entry) =>
          entry.changeKind === "added" &&
          entry.post!.kindName === "BinaryExpression" &&
          /=== "c"/.test(entry.post!.textPreview),
      );
      if (!hasAddedCompare) {
        return `no added BinaryExpression containing '=== "c"'; entries:\n${formatAdded(e)}`;
      }
      return null;
    },
  },
  {
    name: "or-chain-extend-2: AST-style enum check",
    pre: `function isValid(t) {
  return t === "Foo" || t === "Bar";
}`,
    post: `function isValid(t) {
  return t === "Foo" || t === "Bar" || t === "Baz";
}`,
    check: (e) => {
      // Same dual assertion as -1: inner unchanged + outer added.
      const preOrFp = e.find(
        (entry) =>
          entry.pre?.kindName === "BinaryExpression" &&
          /Foo.*\|\|.*Bar/.test(entry.pre.textPreview) &&
          !/Baz/.test(entry.pre.textPreview),
      )?.pre?.fingerprint;
      if (!preOrFp) return "couldn't locate pre 2-clause BinaryExpression";
      const innerUnchanged = e.find(
        (entry) =>
          entry.changeKind === "unchanged" &&
          entry.pre?.fingerprint === preOrFp &&
          entry.post?.kindName === "BinaryExpression",
      );
      if (!innerUnchanged) {
        return `pre 2-clause OR fingerprint ${preOrFp} not unchanged-paired`;
      }
      const hasAddedBaz = e.some(
        (entry) =>
          entry.changeKind === "added" &&
          /Baz/.test(entry.post?.textPreview ?? ""),
      );
      if (!hasAddedBaz) return "no 'added' entry mentioning Baz";
      return null;
    },
  },
];

function formatAdded(entries: DiffEntry[]): string {
  return entries
    .filter((e) => e.changeKind === "added")
    .map((e) => `  + ${e.post!.kindName}: ${e.post!.textPreview}`)
    .join("\n");
}

let pass = 0;
let fail = 0;
for (const f of fixtures) {
  const entries = computeFileDiff(f.pre, f.post);
  const reason = f.check(entries);
  const summary = summarize(entries);
  if (reason === null) {
    console.log(`PASS  ${f.name}  ${JSON.stringify(summary)}`);
    pass++;
  } else {
    console.log(`FAIL  ${f.name}  ${JSON.stringify(summary)}`);
    console.log(`      reason: ${reason}`);
    if (process.env["DEBUG"]) {
      for (const entry of entries) {
        const side =
          entry.changeKind === "added"
            ? `+ ${entry.post!.kindName}: ${entry.post!.textPreview}`
            : entry.changeKind === "deleted"
              ? `- ${entry.pre!.kindName}: ${entry.pre!.textPreview}`
              : entry.changeKind === "modified"
                ? `~ ${entry.pre!.kindName}: ${entry.pre!.textPreview} → ${entry.post!.textPreview}`
                : `= ${entry.pre!.kindName}: ${entry.pre!.textPreview}`;
        console.log(`      ${entry.changeKind.padEnd(10)} ${side}`);
      }
    }
    fail++;
  }
}

console.log();
console.log(`${pass}/${pass + fail} passed`);
if (fail > 0) console.log(`(set DEBUG=1 to see per-entry classification)`);
process.exit(fail > 0 ? 1 : 0);
