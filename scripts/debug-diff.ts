#!/usr/bin/env tsx
/**
 * Smoke validation for src/sast/diff.ts.
 *
 * Per advisor (2026-04-27): 10 of each beats 30 random. Three fixture
 * shapes that the diff classifier MUST get right:
 *
 *   1. pure-addition    — fix adds a guard before existing code
 *   2. in-place-mod     — fix swaps an operator or literal
 *   3. or-chain-extend  — fix appends a new clause to an OR-chain (the
 *                          enum-disjunction principle's mining target)
 *
 * Each fixture asserts an expected (unchanged, modified, added, deleted)
 * shape — not exact counts, since ts-morph's child decomposition is
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
      // The "1" → "2" change: NumericLiteral fingerprints differ, but the
      // numeric literal should occupy the same role under the same parent
      // shape... well, the parent BinaryExpression's fingerprint also
      // changes. So in step 1 nothing pairs at the BinaryExpr level.
      // In step 2, the BinaryExpression pairs by (parent=ReturnStatement-pre-fp, ordinal=0, kind=BinaryExpression).
      // Wait: ReturnStatement fingerprints differ too because they include
      // their child's fingerprints recursively. So the parent-fp won't
      // match. This climbs all the way up to FunctionDeclaration which
      // also won't match. So the modification "modified" classification
      // will only land at the top of the modification chain.
      //
      // What's guaranteed: at least one "modified" entry. The exact level
      // depends on ts-morph's tree shape — accept any "modified".
      const summary = summarize(e);
      if (summary.modified === 0) return "no 'modified' entries";
      if (summary.added === 0) return "no 'added' entries (the new literal is unmatched)";
      if (summary.deleted === 0) return "no 'deleted' entries (the old literal is unmatched)";
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
  {
    name: "or-chain-extend-1: enum disjunction",
    pre: `function check(x) {
  return x === "a" || x === "b";
}`,
    post: `function check(x) {
  return x === "a" || x === "b" || x === "c";
}`,
    check: (e) => {
      // Pre BinaryExpr (a || b) fingerprint should appear as an unchanged
      // subtree inside post (post parses as (a||b) || c, so the inner
      // BinaryExpression has the same fingerprint as the entire pre
      // BinaryExpression).
      const summary = summarize(e);
      if (!hasUnchangedCount(e, 5)) {
        return `expected ≥5 unchanged (function shell + inner OR shape preserved); got ${summary.unchanged}`;
      }
      // The new "x === c" comparison must show up as added.
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
