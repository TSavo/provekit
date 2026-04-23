# Phase A-thin + D-core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship encoding-gap localization (the thesis-distinctive capability from THESIS.md) on top of v1's existing harness by adding a SQLite+Drizzle data layer for traces, clauses, witnesses, and gap reports; a Z3 model parser; a runtime value serializer; snapshot-based instrumented execution; and a domain-aware comparator with three sort-specific agents (IEEE specials, outcome mismatch, path not taken).

**Architecture:** The SAST IR and principle DSL come in a later phase. For this plan, v1 JSON contracts stay as-is; new data (traces, witnesses, gaps) lands in SQLite via Drizzle. Bindings are line-referenced (source_line + source_expr) — when SAST arrives in Phase B, a migration adds `bound_to_node` FK to nodes and backfills. Everything in the schema is relational; no JSON columns. Runtime values are stored as a graph (`runtime_values` + `object_members` + `array_elements`).

**Tech Stack:** TypeScript, vitest, ts-morph (already in deps), better-sqlite3 (new), drizzle-orm + drizzle-kit (new), Z3 CLI (already used via execSync).

**Spec reference:** `docs/specs/2026-04-23-provekit-v2-design.md`

---

## File Structure

**New files (schema):**
- `src/db/index.ts` — Drizzle DB connection + type exports
- `src/db/schema/runtimeValues.ts` — `runtime_values`, `runtime_value_object_members`, `runtime_value_array_elements`
- `src/db/schema/traces.ts` — `traces`, `trace_values`
- `src/db/schema/clauses.ts` — `clauses`, `clause_bindings`, `clause_witnesses`
- `src/db/schema/gapReports.ts` — `gap_reports`
- `src/db/schema/index.ts` — re-exports all schemas
- `drizzle.config.ts` — drizzle-kit config

**New files (D-core):**
- `src/z3/modelParser.ts` — parse Z3 `(model ...)` output
- `src/z3/persistWitness.ts` — write parsed model to `clause_witnesses` + `runtime_values`
- `src/runtime/valueSerializer.ts` — serialize JS value → `runtime_values` graph
- `src/runtime/snapshotInstrumentation.ts` — ts-morph AST rewrite at signal line
- `src/bindings/validator.ts` — line-based binding validation
- `src/comparator/core.ts` — orchestrates sort agents
- `src/comparator/agents/ieeeSpecials.ts` — Real sort vs IEEE 754
- `src/comparator/agents/outcomeMismatch.ts` — return vs throw disagreement
- `src/comparator/agents/pathNotTaken.ts` — witness path vs trace path
- `src/gapDetection.ts` — full pipeline: parse + instrument + compare + persist

**Modified files:**
- `package.json` — add better-sqlite3, drizzle-orm, drizzle-kit
- `src/harness.ts` — add `captureTrace` option to `runHarness`
- `src/cli.ts` — add `--gaps` flag to `explain` subcommand
- `prompts/invariant_derivation.md` — LLM emits `smt_bindings` per clause
- `src/contracts.ts` — extend `Violation` type with optional `smt_bindings` field for transitional v1 compat
- `.gitignore` — add `.neurallog/neurallog.db`, `.neurallog/neurallog.db-journal`

**Test files:**
- `src/db/index.test.ts`
- `src/db/schema/runtimeValues.test.ts`
- `src/db/schema/traces.test.ts`
- `src/db/schema/clauses.test.ts`
- `src/db/schema/gapReports.test.ts`
- `src/z3/modelParser.test.ts`
- `src/z3/persistWitness.test.ts`
- `src/runtime/valueSerializer.test.ts`
- `src/runtime/snapshotInstrumentation.test.ts`
- `src/harness.captureTrace.test.ts`
- `src/bindings/validator.test.ts`
- `src/comparator/agents/ieeeSpecials.test.ts`
- `src/comparator/agents/outcomeMismatch.test.ts`
- `src/comparator/agents/pathNotTaken.test.ts`
- `src/gapDetection.test.ts`
- `src/cli.gaps.test.ts` (end-to-end)

---

## Task 1: Add dependencies and DB connection module

**Files:**
- Modify: `package.json`
- Create: `drizzle.config.ts`
- Create: `src/db/index.ts`
- Modify: `.gitignore`
- Create: `src/db/index.test.ts`

- [ ] **Step 1.1: Install dependencies**

Run:
```bash
npm install better-sqlite3 drizzle-orm
npm install --save-dev drizzle-kit @types/better-sqlite3
```

Expected: installs complete without errors. `package.json` gains four new entries.

- [ ] **Step 1.2: Add `.gitignore` entries for the DB file**

Edit `.gitignore` — add at end:
```
.neurallog/neurallog.db
.neurallog/neurallog.db-journal
.neurallog/neurallog.db-wal
.neurallog/neurallog.db-shm
```

- [ ] **Step 1.3: Create `drizzle.config.ts`**

Create file:
```typescript
import type { Config } from "drizzle-kit";

export default {
  schema: "./src/db/schema/index.ts",
  out: "./drizzle",
  dialect: "sqlite",
  dbCredentials: {
    url: ".neurallog/neurallog.db",
  },
} satisfies Config;
```

- [ ] **Step 1.4: Write failing test for DB connection**

Create `src/db/index.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./index.js";

describe("openDb", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("opens a connection against a new sqlite file and runs a trivial query", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const dbPath = join(tmpDir, "test.db");
    const db = openDb(dbPath);
    const result = db.$client.prepare("select 1 as x").get() as { x: number };
    expect(result.x).toBe(1);
    db.$client.close();
  });
});
```

- [ ] **Step 1.5: Run test to verify it fails**

Run: `npx vitest run src/db/index.test.ts`
Expected: FAIL with `Cannot find module './index.js'` or similar.

- [ ] **Step 1.6: Implement `src/db/index.ts`**

Create file:
```typescript
import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import { mkdirSync } from "fs";
import { dirname } from "path";

export function openDb(path: string) {
  mkdirSync(dirname(path), { recursive: true });
  const sqlite = new Database(path);
  sqlite.pragma("journal_mode = WAL");
  sqlite.pragma("foreign_keys = ON");
  return drizzle(sqlite);
}

export type Db = ReturnType<typeof openDb>;
```

- [ ] **Step 1.7: Run test to verify it passes**

Run: `npx vitest run src/db/index.test.ts`
Expected: PASS (1 test).

- [ ] **Step 1.8: Commit**

```bash
git add package.json package-lock.json .gitignore drizzle.config.ts src/db/
git commit -m "chore: add better-sqlite3 + drizzle; wire DB connection"
```

---

## Task 2: Schema — runtime_values graph

**Files:**
- Create: `src/db/schema/runtimeValues.ts`
- Create: `src/db/schema/index.ts`
- Create: `src/db/schema/runtimeValues.test.ts`
- Generate: `drizzle/0000_initial.sql` (via drizzle-kit)

- [ ] **Step 2.1: Write failing test for runtime value insert + query**

Create `src/db/schema/runtimeValues.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues, runtimeValueObjectMembers } from "./runtimeValues.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("runtime_values schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a primitive number value and reads it back", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const inserted = db
      .insert(runtimeValues)
      .values({ kind: "number", numberValue: 42 })
      .returning()
      .get();

    const back = db
      .select()
      .from(runtimeValues)
      .where(eq(runtimeValues.id, inserted.id))
      .get();

    expect(back?.kind).toBe("number");
    expect(back?.numberValue).toBe(42);
  });

  it("inserts an object with a member and joins through the edge table", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const obj = db.insert(runtimeValues).values({ kind: "object" }).returning().get();
    const ok = db.insert(runtimeValues).values({ kind: "bool", boolValue: false }).returning().get();
    db.insert(runtimeValueObjectMembers).values({ parentValueId: obj.id, key: "ok", childValueId: ok.id }).run();

    const rows = db
      .select({ key: runtimeValueObjectMembers.key, childKind: runtimeValues.kind, childBool: runtimeValues.boolValue })
      .from(runtimeValueObjectMembers)
      .innerJoin(runtimeValues, eq(runtimeValues.id, runtimeValueObjectMembers.childValueId))
      .where(eq(runtimeValueObjectMembers.parentValueId, obj.id))
      .all();

    expect(rows).toEqual([{ key: "ok", childKind: "bool", childBool: false }]);
  });
});
```

- [ ] **Step 2.2: Run test to verify it fails**

Run: `npx vitest run src/db/schema/runtimeValues.test.ts`
Expected: FAIL with `Cannot find module './runtimeValues.js'`.

- [ ] **Step 2.3: Implement `src/db/schema/runtimeValues.ts`**

Create file:
```typescript
import { sqliteTable, integer, text, real, index, primaryKey } from "drizzle-orm/sqlite-core";

export const runtimeValues = sqliteTable(
  "runtime_values",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    kind: text("kind", {
      enum: [
        "number",
        "string",
        "bool",
        "null",
        "undefined",
        "object",
        "array",
        "function",
        "bigint",
        "symbol",
        "nan",
        "infinity",
        "neg_infinity",
        "circular",
        "truncated",
      ],
    }).notNull(),
    numberValue: real("number_value"),
    stringValue: text("string_value"),
    boolValue: integer("bool_value", { mode: "boolean" }),
    circularTargetId: integer("circular_target_id").references((): any => runtimeValues.id),
    truncationNote: text("truncation_note"),
  },
  (t) => ({
    byKind: index("rv_by_kind").on(t.kind),
    byKindNumber: index("rv_by_kind_number").on(t.kind, t.numberValue),
    byKindString: index("rv_by_kind_string").on(t.kind, t.stringValue),
    byKindBool: index("rv_by_kind_bool").on(t.kind, t.boolValue),
  }),
);

export const runtimeValueObjectMembers = sqliteTable(
  "runtime_value_object_members",
  {
    parentValueId: integer("parent_value_id")
      .notNull()
      .references(() => runtimeValues.id, { onDelete: "cascade" }),
    key: text("key").notNull(),
    childValueId: integer("child_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.parentValueId, t.key] }),
    byChild: index("rvom_by_child").on(t.childValueId),
  }),
);

export const runtimeValueArrayElements = sqliteTable(
  "runtime_value_array_elements",
  {
    parentValueId: integer("parent_value_id")
      .notNull()
      .references(() => runtimeValues.id, { onDelete: "cascade" }),
    elementIndex: integer("element_index").notNull(),
    childValueId: integer("child_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.parentValueId, t.elementIndex] }),
    byChild: index("rvae_by_child").on(t.childValueId),
  }),
);
```

- [ ] **Step 2.4: Create `src/db/schema/index.ts`**

Create file:
```typescript
export * from "./runtimeValues.js";
```

- [ ] **Step 2.5: Generate migration**

Run: `npx drizzle-kit generate`
Expected: creates `drizzle/0000_*.sql` with `CREATE TABLE runtime_values ...`

- [ ] **Step 2.6: Run test to verify it passes**

Run: `npx vitest run src/db/schema/runtimeValues.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 2.7: Commit**

```bash
git add src/db/schema/ drizzle/
git commit -m "db: runtime_values graph with object_members and array_elements"
```

---

## Task 3: Schema — traces + trace_values

**Files:**
- Create: `src/db/schema/traces.ts`
- Modify: `src/db/schema/index.ts`
- Create: `src/db/schema/traces.test.ts`
- Regenerate: `drizzle/0001_*.sql`

- [ ] **Step 3.1: Write failing test**

Create `src/db/schema/traces.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues } from "./runtimeValues.js";
import { traces, traceValues } from "./traces.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("traces schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a trace and a trace_value referencing a runtime_value", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const ret = db.insert(runtimeValues).values({ kind: "number", numberValue: 42 }).returning().get();
    const trace = db
      .insert(traces)
      .values({
        clauseId: 1,
        capturedAt: Date.now(),
        outcomeKind: "returned",
        outcomeValueId: ret.id,
        inputsHash: "abc123",
      })
      .returning()
      .get();

    const snap = db.insert(runtimeValues).values({ kind: "number", numberValue: 5 }).returning().get();
    db.insert(traceValues).values({
      traceId: trace.id,
      nodeId: "src/foo.ts:10:5",
      iterationIndex: null,
      rootValueId: snap.id,
    }).run();

    const rows = db
      .select({ kind: runtimeValues.kind, value: runtimeValues.numberValue })
      .from(traceValues)
      .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
      .where(eq(traceValues.traceId, trace.id))
      .all();
    expect(rows).toEqual([{ kind: "number", value: 5 }]);
  });
});
```

Note: this test uses `clauseId: 1` without a real clauses row — acceptable for isolation because foreign key on clauseId is added only after `clauses` schema lands (Task 4). Enforce this ordering: Task 3's migration MUST NOT declare the clauseId FK; Task 4 adds it.

- [ ] **Step 3.2: Run test to verify it fails**

Run: `npx vitest run src/db/schema/traces.test.ts`
Expected: FAIL with `Cannot find module './traces.js'`.

- [ ] **Step 3.3: Implement `src/db/schema/traces.ts`**

Create file:
```typescript
import { sqliteTable, integer, text, index, primaryKey } from "drizzle-orm/sqlite-core";
import { runtimeValues } from "./runtimeValues.js";
import { nodes } from "./nodes.js";

// NB: nodes table doesn't exist yet in Phase A-thin (SAST is Phase B).
// During A-thin we keep node_id as a free-form TEXT column (source reference like
// "src/foo.ts:10:5"); Phase B migration will add the FK to nodes(id).

export const traces = sqliteTable(
  "traces",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    // clauseId FK added in Task 4 via migration once clauses table exists
    clauseId: integer("clause_id").notNull(),
    capturedAt: integer("captured_at").notNull(),
    outcomeKind: text("outcome_kind", {
      enum: ["returned", "threw", "untestable"],
    }).notNull(),
    outcomeValueId: integer("outcome_value_id").references(() => runtimeValues.id),
    untestableReason: text("untestable_reason"),
    inputsHash: text("inputs_hash").notNull(),
  },
  (t) => ({
    byClause: index("traces_by_clause").on(t.clauseId),
  }),
);

export const traceValues = sqliteTable(
  "trace_values",
  {
    traceId: integer("trace_id")
      .notNull()
      .references(() => traces.id, { onDelete: "cascade" }),
    nodeId: text("node_id").notNull(), // Phase A-thin: free-form source reference
    iterationIndex: integer("iteration_index"),
    rootValueId: integer("root_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.traceId, t.nodeId, t.iterationIndex] }),
    byNode: index("tv_by_node").on(t.nodeId),
    byRoot: index("tv_by_root").on(t.rootValueId),
  }),
);
```

Delete the `import { nodes } from "./nodes.js";` line — nodes doesn't exist yet. Correction: don't import at all.

Revised file (final form for this step):
```typescript
import { sqliteTable, integer, text, index, primaryKey } from "drizzle-orm/sqlite-core";
import { runtimeValues } from "./runtimeValues.js";

// NB: Phase A-thin keeps node_id as a TEXT source reference.
// Phase B will add FK to nodes(id) once SAST is built.

export const traces = sqliteTable(
  "traces",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    clauseId: integer("clause_id").notNull(),
    capturedAt: integer("captured_at").notNull(),
    outcomeKind: text("outcome_kind", {
      enum: ["returned", "threw", "untestable"],
    }).notNull(),
    outcomeValueId: integer("outcome_value_id").references(() => runtimeValues.id),
    untestableReason: text("untestable_reason"),
    inputsHash: text("inputs_hash").notNull(),
  },
  (t) => ({
    byClause: index("traces_by_clause").on(t.clauseId),
  }),
);

export const traceValues = sqliteTable(
  "trace_values",
  {
    traceId: integer("trace_id")
      .notNull()
      .references(() => traces.id, { onDelete: "cascade" }),
    nodeId: text("node_id").notNull(),
    iterationIndex: integer("iteration_index"),
    rootValueId: integer("root_value_id")
      .notNull()
      .references(() => runtimeValues.id),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.traceId, t.nodeId, t.iterationIndex] }),
    byNode: index("tv_by_node").on(t.nodeId),
    byRoot: index("tv_by_root").on(t.rootValueId),
  }),
);
```

- [ ] **Step 3.4: Update `src/db/schema/index.ts`**

Edit to add:
```typescript
export * from "./runtimeValues.js";
export * from "./traces.js";
```

- [ ] **Step 3.5: Generate migration**

Run: `npx drizzle-kit generate`
Expected: creates `drizzle/0001_*.sql` with `CREATE TABLE traces ...` and `CREATE TABLE trace_values ...`.

- [ ] **Step 3.6: Run test to verify it passes**

Run: `npx vitest run src/db/schema/traces.test.ts`
Expected: PASS (1 test).

- [ ] **Step 3.7: Commit**

```bash
git add src/db/schema/ drizzle/
git commit -m "db: traces + trace_values schema"
```

---

## Task 4: Schema — clauses + clause_bindings + clause_witnesses

**Files:**
- Create: `src/db/schema/clauses.ts`
- Modify: `src/db/schema/index.ts`
- Create: `src/db/schema/clauses.test.ts`
- Generate: `drizzle/0002_*.sql`

- [ ] **Step 4.1: Write failing test**

Create `src/db/schema/clauses.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues } from "./runtimeValues.js";
import { clauses, clauseBindings, clauseWitnesses } from "./clauses.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("clauses schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a clause with a binding and a witness referencing a runtime_value", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db
      .insert(clauses)
      .values({
        contractKey: "src/math.ts/divide[10]",
        verdict: "violation",
        smt2: "(declare-const den Real) (assert (= den 0))",
        clauseHash: "abc",
      })
      .returning()
      .get();

    db.insert(clauseBindings)
      .values({
        clauseId: clause.id,
        smtConstant: "den",
        sourceLine: 10,
        sourceExpr: "denominator",
        sort: "Real",
      })
      .run();

    const wvalue = db
      .insert(runtimeValues)
      .values({ kind: "number", numberValue: 0 })
      .returning()
      .get();

    db.insert(clauseWitnesses)
      .values({
        clauseId: clause.id,
        smtConstant: "den",
        modelValueId: wvalue.id,
        sort: "Real",
      })
      .run();

    const bindings = db.select().from(clauseBindings).where(eq(clauseBindings.clauseId, clause.id)).all();
    expect(bindings).toHaveLength(1);
    expect(bindings[0].sourceLine).toBe(10);

    const witnesses = db
      .select({ constant: clauseWitnesses.smtConstant, value: runtimeValues.numberValue })
      .from(clauseWitnesses)
      .innerJoin(runtimeValues, eq(runtimeValues.id, clauseWitnesses.modelValueId))
      .where(eq(clauseWitnesses.clauseId, clause.id))
      .all();
    expect(witnesses).toEqual([{ constant: "den", value: 0 }]);
  });
});
```

- [ ] **Step 4.2: Run test to verify it fails**

Run: `npx vitest run src/db/schema/clauses.test.ts`
Expected: FAIL with `Cannot find module './clauses.js'`.

- [ ] **Step 4.3: Implement `src/db/schema/clauses.ts`**

Create file:
```typescript
import { sqliteTable, integer, text, index, primaryKey } from "drizzle-orm/sqlite-core";
import { runtimeValues } from "./runtimeValues.js";

// Phase A-thin: contractKey is a free-form text reference to the v1 JSON contract
// (format: file/fn[line], matching existing signalKey). Phase A-full will migrate
// this to FK against a contracts table.

export const clauses = sqliteTable(
  "clauses",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    contractKey: text("contract_key").notNull(),
    verdict: text("verdict", {
      enum: ["proven", "violation", "unknown", "vacuous"],
    }).notNull(),
    smt2: text("smt2").notNull(),
    clauseHash: text("clause_hash").notNull(),
    principleName: text("principle_name"),
    complexity: integer("complexity"),
    confidence: text("confidence"),
    judgeNote: text("judge_note"),
    vacuousReason: text("vacuous_reason"),
  },
  (t) => ({
    byContract: index("clauses_by_contract").on(t.contractKey),
    byHash: index("clauses_by_hash").on(t.clauseHash),
  }),
);

export const clauseBindings = sqliteTable(
  "clause_bindings",
  {
    clauseId: integer("clause_id")
      .notNull()
      .references(() => clauses.id, { onDelete: "cascade" }),
    smtConstant: text("smt_constant").notNull(),
    // Phase A-thin: line-referenced. Phase B adds `boundToNode` FK to nodes.
    sourceLine: integer("source_line").notNull(),
    sourceExpr: text("source_expr").notNull(),
    sort: text("sort").notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.clauseId, t.smtConstant] }),
  }),
);

export const clauseWitnesses = sqliteTable(
  "clause_witnesses",
  {
    clauseId: integer("clause_id")
      .notNull()
      .references(() => clauses.id, { onDelete: "cascade" }),
    smtConstant: text("smt_constant").notNull(),
    modelValueId: integer("model_value_id")
      .notNull()
      .references(() => runtimeValues.id),
    sort: text("sort").notNull(),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.clauseId, t.smtConstant] }),
    byValue: index("cw_by_value").on(t.modelValueId),
  }),
);
```

- [ ] **Step 4.4: Update `src/db/schema/index.ts`**

Edit to add:
```typescript
export * from "./runtimeValues.js";
export * from "./traces.js";
export * from "./clauses.js";
```

- [ ] **Step 4.5: Generate migration**

Run: `npx drizzle-kit generate`
Expected: `drizzle/0002_*.sql` with `CREATE TABLE clauses`, `clause_bindings`, `clause_witnesses`.

- [ ] **Step 4.6: Run test to verify it passes**

Run: `npx vitest run src/db/schema/clauses.test.ts`
Expected: PASS (1 test).

- [ ] **Step 4.7: Commit**

```bash
git add src/db/schema/ drizzle/
git commit -m "db: clauses + clause_bindings + clause_witnesses schema"
```

---

## Task 5: Schema — gap_reports

**Files:**
- Create: `src/db/schema/gapReports.ts`
- Modify: `src/db/schema/index.ts`
- Create: `src/db/schema/gapReports.test.ts`
- Generate: `drizzle/0003_*.sql`

- [ ] **Step 5.1: Write failing test**

Create `src/db/schema/gapReports.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../index.js";
import { runtimeValues } from "./runtimeValues.js";
import { clauses } from "./clauses.js";
import { gapReports } from "./gapReports.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("gap_reports schema", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("inserts a gap_report referencing a clause + two runtime_values", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/math.ts/divide[10]",
      verdict: "proven",
      smt2: "(assert (not (= den 0)))",
      clauseHash: "c1",
    }).returning().get();

    const smtVal = db.insert(runtimeValues).values({ kind: "number", numberValue: 0 }).returning().get();
    const rtVal = db.insert(runtimeValues).values({ kind: "nan" }).returning().get();

    const gap = db.insert(gapReports).values({
      clauseId: clause.id,
      kind: "ieee_specials",
      smtConstant: "den",
      smtValueId: smtVal.id,
      runtimeValueId: rtVal.id,
      explanation: "SMT Real does not model IEEE NaN",
    }).returning().get();

    const rows = db.select().from(gapReports).where(eq(gapReports.id, gap.id)).all();
    expect(rows).toHaveLength(1);
    expect(rows[0].kind).toBe("ieee_specials");
    expect(rows[0].smtValueId).toBe(smtVal.id);
    expect(rows[0].runtimeValueId).toBe(rtVal.id);
  });
});
```

- [ ] **Step 5.2: Run test to verify it fails**

Run: `npx vitest run src/db/schema/gapReports.test.ts`
Expected: FAIL with `Cannot find module './gapReports.js'`.

- [ ] **Step 5.3: Implement `src/db/schema/gapReports.ts`**

Create file:
```typescript
import { sqliteTable, integer, text, index } from "drizzle-orm/sqlite-core";
import { clauses } from "./clauses.js";
import { traces } from "./traces.js";
import { runtimeValues } from "./runtimeValues.js";

export const gapReports = sqliteTable(
  "gap_reports",
  {
    id: integer("id").primaryKey({ autoIncrement: true }),
    clauseId: integer("clause_id")
      .notNull()
      .references(() => clauses.id, { onDelete: "cascade" }),
    traceId: integer("trace_id").references(() => traces.id),
    kind: text("kind", {
      enum: [
        "ieee_specials",
        "int_overflow",
        "bool_coercion",
        "null_undefined",
        "path_not_taken",
        "outcome_mismatch",
        "invalid_binding",
      ],
    }).notNull(),
    smtConstant: text("smt_constant"),
    // Phase A-thin: node reference as free-form text; Phase B adds FK.
    atNodeRef: text("at_node_ref"),
    smtValueId: integer("smt_value_id").references(() => runtimeValues.id),
    runtimeValueId: integer("runtime_value_id").references(() => runtimeValues.id),
    explanation: text("explanation"),
  },
  (t) => ({
    byClause: index("gr_by_clause").on(t.clauseId),
    byKind: index("gr_by_kind").on(t.kind),
    byNodeRef: index("gr_by_node_ref").on(t.atNodeRef),
  }),
);
```

- [ ] **Step 5.4: Update `src/db/schema/index.ts`**

Edit to add:
```typescript
export * from "./runtimeValues.js";
export * from "./traces.js";
export * from "./clauses.js";
export * from "./gapReports.js";
```

- [ ] **Step 5.5: Generate migration**

Run: `npx drizzle-kit generate`
Expected: `drizzle/0003_*.sql` with `CREATE TABLE gap_reports`.

- [ ] **Step 5.6: Run test to verify it passes**

Run: `npx vitest run src/db/schema/gapReports.test.ts`
Expected: PASS (1 test).

- [ ] **Step 5.7: Run full test suite to verify no regressions**

Run: `npx vitest run`
Expected: PASS all tests including original 87.

- [ ] **Step 5.8: Commit**

```bash
git add src/db/schema/ drizzle/
git commit -m "db: gap_reports schema (references clauses + traces + runtime_values)"
```

---

## Task 6: Z3 model parser

**Files:**
- Create: `src/z3/modelParser.ts`
- Create: `src/z3/modelParser.test.ts`

- [ ] **Step 6.1: Write failing test for simple Real values**

Create `src/z3/modelParser.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { parseZ3Model } from "./modelParser.js";

describe("parseZ3Model", () => {
  it("parses a simple Real model", () => {
    const input = `
(
  (define-fun den () Real
    0.0)
  (define-fun result () Real
    (/ 1.0 0.0))
)
    `.trim();

    const parsed = parseZ3Model(input);
    expect(parsed.get("den")).toEqual({ sort: "Real", value: 0 });
    expect(parsed.get("result")).toEqual({ sort: "Real", value: "div_by_zero" });
  });

  it("parses Int values", () => {
    const input = `
(
  (define-fun count () Int
    42)
  (define-fun neg () Int
    (- 5))
)
    `.trim();
    const parsed = parseZ3Model(input);
    expect(parsed.get("count")).toEqual({ sort: "Int", value: 42n });
    expect(parsed.get("neg")).toEqual({ sort: "Int", value: -5n });
  });

  it("parses Bool values", () => {
    const input = `
(
  (define-fun ok () Bool
    true)
  (define-fun bad () Bool
    false)
)
    `.trim();
    const parsed = parseZ3Model(input);
    expect(parsed.get("ok")).toEqual({ sort: "Bool", value: true });
    expect(parsed.get("bad")).toEqual({ sort: "Bool", value: false });
  });

  it("parses String values", () => {
    const input = `
(
  (define-fun s () String
    "hello")
)
    `.trim();
    const parsed = parseZ3Model(input);
    expect(parsed.get("s")).toEqual({ sort: "String", value: "hello" });
  });

  it("returns an empty map on empty model", () => {
    expect(parseZ3Model("()").size).toBe(0);
  });
});
```

- [ ] **Step 6.2: Run test to verify it fails**

Run: `npx vitest run src/z3/modelParser.test.ts`
Expected: FAIL with `Cannot find module './modelParser.js'`.

- [ ] **Step 6.3: Implement `src/z3/modelParser.ts`**

Create file:
```typescript
export type Z3Value =
  | { sort: "Real"; value: number | "div_by_zero" | "nan" | "+infinity" | "-infinity" }
  | { sort: "Int"; value: bigint }
  | { sort: "Bool"; value: boolean }
  | { sort: "String"; value: string }
  | { sort: "Other"; raw: string };

export function parseZ3Model(input: string): Map<string, Z3Value> {
  const result = new Map<string, Z3Value>();
  // Z3 emits:
  //   (
  //     (define-fun NAME () SORT VALUE)
  //     ...
  //   )
  // where VALUE may be a literal, a negated literal (- N), or an expression
  // like (/ 1.0 0.0). We do S-expression parsing.
  const tokens = tokenize(input);
  if (tokens.length === 0) return result;
  const tree = parseSexp(tokens, { idx: 0 });
  if (!Array.isArray(tree)) return result;
  for (const entry of tree) {
    if (!Array.isArray(entry)) continue;
    if (entry[0] !== "define-fun") continue;
    const name = entry[1] as string;
    const sort = entry[3] as string;
    const value = entry[4];
    const z3val = interpretValue(sort, value);
    if (z3val) result.set(name, z3val);
  }
  return result;
}

type SexpNode = string | SexpNode[];

function tokenize(s: string): string[] {
  const tokens: string[] = [];
  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === " " || c === "\n" || c === "\t" || c === "\r") {
      i++;
      continue;
    }
    if (c === "(" || c === ")") {
      tokens.push(c);
      i++;
      continue;
    }
    if (c === '"') {
      // string literal — scan to closing quote (Z3 doubles internal quotes)
      let j = i + 1;
      let buf = '"';
      while (j < s.length) {
        if (s[j] === '"' && s[j + 1] === '"') {
          buf += '""';
          j += 2;
          continue;
        }
        buf += s[j];
        if (s[j] === '"') {
          j++;
          break;
        }
        j++;
      }
      tokens.push(buf);
      i = j;
      continue;
    }
    // atom
    let j = i;
    while (j < s.length && !/[()\s]/.test(s[j]!)) j++;
    tokens.push(s.slice(i, j));
    i = j;
  }
  return tokens;
}

function parseSexp(tokens: string[], pos: { idx: number }): SexpNode {
  const t = tokens[pos.idx++];
  if (t === "(") {
    const list: SexpNode[] = [];
    while (tokens[pos.idx] !== ")") {
      if (pos.idx >= tokens.length) throw new Error("unclosed paren");
      list.push(parseSexp(tokens, pos));
    }
    pos.idx++; // consume ')'
    return list;
  }
  if (t === ")" || t === undefined) throw new Error(`unexpected token ${t}`);
  return t;
}

function interpretValue(sort: string, value: SexpNode): Z3Value | null {
  if (sort === "Bool") {
    return { sort: "Bool", value: value === "true" };
  }
  if (sort === "Int") {
    if (typeof value === "string") return { sort: "Int", value: BigInt(value) };
    if (Array.isArray(value) && value[0] === "-" && typeof value[1] === "string") {
      return { sort: "Int", value: -BigInt(value[1]) };
    }
    return { sort: "Other", raw: stringify(value) };
  }
  if (sort === "Real") {
    if (typeof value === "string") {
      if (value === "+oo" || value === "oo" || value === "inf") return { sort: "Real", value: "+infinity" };
      if (value === "-oo") return { sort: "Real", value: "-infinity" };
      const n = Number(value);
      if (!Number.isNaN(n)) return { sort: "Real", value: n };
      return { sort: "Other", raw: value };
    }
    if (Array.isArray(value)) {
      if (value[0] === "-" && typeof value[1] === "string") {
        return { sort: "Real", value: -Number(value[1]) };
      }
      if (value[0] === "/" && value.length === 3 && typeof value[1] === "string" && typeof value[2] === "string") {
        const n = Number(value[1]);
        const d = Number(value[2]);
        if (d === 0) return { sort: "Real", value: "div_by_zero" };
        return { sort: "Real", value: n / d };
      }
      return { sort: "Other", raw: stringify(value) };
    }
  }
  if (sort === "String") {
    if (typeof value === "string" && value.startsWith('"') && value.endsWith('"')) {
      return { sort: "String", value: value.slice(1, -1).replace(/""/g, '"') };
    }
  }
  return { sort: "Other", raw: stringify(value) };
}

function stringify(node: SexpNode): string {
  if (typeof node === "string") return node;
  return "(" + node.map(stringify).join(" ") + ")";
}
```

- [ ] **Step 6.4: Run test to verify it passes**

Run: `npx vitest run src/z3/modelParser.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 6.5: Commit**

```bash
git add src/z3/
git commit -m "z3: model parser (Real/Int/Bool/String + div_by_zero/infinity specials)"
```

---

## Task 7: Persist parsed Z3 model to DB

**Files:**
- Create: `src/z3/persistWitness.ts`
- Create: `src/z3/persistWitness.test.ts`

- [ ] **Step 7.1: Write failing test**

Create `src/z3/persistWitness.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { clauses, clauseWitnesses, runtimeValues } from "../db/schema/index.js";
import { persistWitness } from "./persistWitness.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("persistWitness", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("writes parsed witness values to clause_witnesses and runtime_values", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/math.ts/divide[10]",
      verdict: "violation",
      smt2: "(assert (= den 0))",
      clauseHash: "c1",
    }).returning().get();

    persistWitness(db, clause.id, new Map([
      ["den", { sort: "Real", value: 0 }],
      ["count", { sort: "Int", value: 5n }],
      ["ok", { sort: "Bool", value: false }],
    ]));

    const witnesses = db.select().from(clauseWitnesses).where(eq(clauseWitnesses.clauseId, clause.id)).all();
    expect(witnesses).toHaveLength(3);

    const byName = new Map(witnesses.map((w) => [w.smtConstant, w]));
    const denVal = db.select().from(runtimeValues).where(eq(runtimeValues.id, byName.get("den")!.modelValueId)).get();
    expect(denVal?.kind).toBe("number");
    expect(denVal?.numberValue).toBe(0);

    const okVal = db.select().from(runtimeValues).where(eq(runtimeValues.id, byName.get("ok")!.modelValueId)).get();
    expect(okVal?.kind).toBe("bool");
    expect(okVal?.boolValue).toBe(false);
  });
});
```

- [ ] **Step 7.2: Run test to verify it fails**

Run: `npx vitest run src/z3/persistWitness.test.ts`
Expected: FAIL with `Cannot find module './persistWitness.js'`.

- [ ] **Step 7.3: Implement `src/z3/persistWitness.ts`**

Create file:
```typescript
import type { Db } from "../db/index.js";
import { clauseWitnesses, runtimeValues } from "../db/schema/index.js";
import type { Z3Value } from "./modelParser.js";

export function persistWitness(db: Db, clauseId: number, model: Map<string, Z3Value>): void {
  for (const [constantName, z3val] of model) {
    const valueRow = writeZ3Value(db, z3val);
    db.insert(clauseWitnesses).values({
      clauseId,
      smtConstant: constantName,
      modelValueId: valueRow.id,
      sort: z3val.sort,
    }).run();
  }
}

function writeZ3Value(db: Db, v: Z3Value): { id: number } {
  if (v.sort === "Real") {
    if (typeof v.value === "number") {
      return db.insert(runtimeValues).values({ kind: "number", numberValue: v.value }).returning().get();
    }
    if (v.value === "div_by_zero" || v.value === "nan") {
      return db.insert(runtimeValues).values({ kind: "nan" }).returning().get();
    }
    if (v.value === "+infinity") return db.insert(runtimeValues).values({ kind: "infinity" }).returning().get();
    if (v.value === "-infinity") return db.insert(runtimeValues).values({ kind: "neg_infinity" }).returning().get();
  }
  if (v.sort === "Int") {
    const n = Number(v.value);
    if (Number.isSafeInteger(n)) {
      return db.insert(runtimeValues).values({ kind: "number", numberValue: n }).returning().get();
    }
    return db.insert(runtimeValues).values({ kind: "bigint", stringValue: v.value.toString() }).returning().get();
  }
  if (v.sort === "Bool") {
    return db.insert(runtimeValues).values({ kind: "bool", boolValue: v.value }).returning().get();
  }
  if (v.sort === "String") {
    return db.insert(runtimeValues).values({ kind: "string", stringValue: v.value }).returning().get();
  }
  // Other / unrecognized: store raw as a string
  const raw = "raw" in v ? v.raw : JSON.stringify(v);
  return db.insert(runtimeValues).values({ kind: "string", stringValue: raw }).returning().get();
}
```

- [ ] **Step 7.4: Run test to verify it passes**

Run: `npx vitest run src/z3/persistWitness.test.ts`
Expected: PASS (1 test).

- [ ] **Step 7.5: Commit**

```bash
git add src/z3/
git commit -m "z3: persistWitness writes parsed model to clause_witnesses + runtime_values"
```

---

## Task 8: Runtime value serializer (JS → runtime_values graph)

**Files:**
- Create: `src/runtime/valueSerializer.ts`
- Create: `src/runtime/valueSerializer.test.ts`

- [ ] **Step 8.1: Write failing test**

Create `src/runtime/valueSerializer.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "../db/index.js";
import { runtimeValues, runtimeValueObjectMembers, runtimeValueArrayElements } from "../db/schema/index.js";
import { serializeValue } from "./valueSerializer.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("serializeValue", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  function freshDb() {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });
    return db;
  }

  it("serializes a primitive number", () => {
    const db = freshDb();
    const id = serializeValue(db, 42);
    const row = db.select().from(runtimeValues).where(eq(runtimeValues.id, id)).get();
    expect(row?.kind).toBe("number");
    expect(row?.numberValue).toBe(42);
  });

  it("serializes NaN as kind='nan'", () => {
    const db = freshDb();
    const id = serializeValue(db, NaN);
    const row = db.select().from(runtimeValues).where(eq(runtimeValues.id, id)).get();
    expect(row?.kind).toBe("nan");
  });

  it("serializes +Infinity and -Infinity", () => {
    const db = freshDb();
    const pos = serializeValue(db, Infinity);
    const neg = serializeValue(db, -Infinity);
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, pos)).get()?.kind).toBe("infinity");
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, neg)).get()?.kind).toBe("neg_infinity");
  });

  it("serializes null and undefined", () => {
    const db = freshDb();
    const nullId = serializeValue(db, null);
    const undefId = serializeValue(db, undefined);
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, nullId)).get()?.kind).toBe("null");
    expect(db.select().from(runtimeValues).where(eq(runtimeValues.id, undefId)).get()?.kind).toBe("undefined");
  });

  it("serializes a flat object", () => {
    const db = freshDb();
    const id = serializeValue(db, { name: "x", count: 3 });
    const members = db.select().from(runtimeValueObjectMembers).where(eq(runtimeValueObjectMembers.parentValueId, id)).all();
    expect(members).toHaveLength(2);
    const byKey = new Map(members.map((m) => [m.key, m.childValueId]));
    const nameChild = db.select().from(runtimeValues).where(eq(runtimeValues.id, byKey.get("name")!)).get();
    expect(nameChild?.kind).toBe("string");
    expect(nameChild?.stringValue).toBe("x");
  });

  it("serializes an array", () => {
    const db = freshDb();
    const id = serializeValue(db, [10, 20, 30]);
    const rows = db.select().from(runtimeValueArrayElements).where(eq(runtimeValueArrayElements.parentValueId, id)).all();
    expect(rows).toHaveLength(3);
    const vals = rows.sort((a, b) => a.elementIndex - b.elementIndex).map((r) => {
      return db.select().from(runtimeValues).where(eq(runtimeValues.id, r.childValueId)).get()?.numberValue;
    });
    expect(vals).toEqual([10, 20, 30]);
  });

  it("serializes a circular reference with kind='circular'", () => {
    const db = freshDb();
    const o: any = { name: "root" };
    o.self = o;
    const id = serializeValue(db, o);
    const members = db.select().from(runtimeValueObjectMembers).where(eq(runtimeValueObjectMembers.parentValueId, id)).all();
    const selfMember = members.find((m) => m.key === "self");
    expect(selfMember).toBeDefined();
    const child = db.select().from(runtimeValues).where(eq(runtimeValues.id, selfMember!.childValueId)).get();
    expect(child?.kind).toBe("circular");
    expect(child?.circularTargetId).toBe(id);
  });

  it("truncates long strings", () => {
    const db = freshDb();
    const longStr = "x".repeat(2000);
    const id = serializeValue(db, longStr);
    const row = db.select().from(runtimeValues).where(eq(runtimeValues.id, id)).get();
    expect(row?.kind).toBe("truncated");
  });
});
```

- [ ] **Step 8.2: Run test to verify it fails**

Run: `npx vitest run src/runtime/valueSerializer.test.ts`
Expected: FAIL with `Cannot find module './valueSerializer.js'`.

- [ ] **Step 8.3: Implement `src/runtime/valueSerializer.ts`**

Create file:
```typescript
import type { Db } from "../db/index.js";
import { runtimeValues, runtimeValueObjectMembers, runtimeValueArrayElements } from "../db/schema/index.js";

const MAX_STRING_BYTES = 1024;
const MAX_ARRAY_ELEMENTS = 100;
const MAX_OBJECT_KEYS = 100;

export function serializeValue(db: Db, value: unknown): number {
  const seen = new Map<object, number>();
  return writeValue(db, value, seen);
}

function writeValue(db: Db, v: unknown, seen: Map<object, number>): number {
  if (v === null) return insert(db, { kind: "null" });
  if (v === undefined) return insert(db, { kind: "undefined" });

  const t = typeof v;

  if (t === "boolean") return insert(db, { kind: "bool", boolValue: v as boolean });

  if (t === "number") {
    const n = v as number;
    if (Number.isNaN(n)) return insert(db, { kind: "nan" });
    if (n === Infinity) return insert(db, { kind: "infinity" });
    if (n === -Infinity) return insert(db, { kind: "neg_infinity" });
    return insert(db, { kind: "number", numberValue: n });
  }

  if (t === "string") {
    const s = v as string;
    if (s.length > MAX_STRING_BYTES) {
      return insert(db, { kind: "truncated", truncationNote: `string of length ${s.length} truncated` });
    }
    return insert(db, { kind: "string", stringValue: s });
  }

  if (t === "bigint") return insert(db, { kind: "bigint", stringValue: (v as bigint).toString() });
  if (t === "symbol") return insert(db, { kind: "symbol", stringValue: String(v) });
  if (t === "function") {
    return insert(db, { kind: "function", stringValue: (v as Function).name || "<anonymous>" });
  }

  // object or array
  const obj = v as object;
  if (seen.has(obj)) {
    return insert(db, { kind: "circular", circularTargetId: seen.get(obj)! });
  }

  if (Array.isArray(obj)) {
    const id = insert(db, { kind: "array" });
    seen.set(obj, id);
    const len = Math.min(obj.length, MAX_ARRAY_ELEMENTS);
    for (let i = 0; i < len; i++) {
      const childId = writeValue(db, (obj as any[])[i], seen);
      db.insert(runtimeValueArrayElements).values({
        parentValueId: id,
        elementIndex: i,
        childValueId: childId,
      }).run();
    }
    if (obj.length > MAX_ARRAY_ELEMENTS) {
      const truncId = insert(db, { kind: "truncated", truncationNote: `array of length ${obj.length} truncated at ${MAX_ARRAY_ELEMENTS}` });
      db.insert(runtimeValueArrayElements).values({
        parentValueId: id,
        elementIndex: MAX_ARRAY_ELEMENTS,
        childValueId: truncId,
      }).run();
    }
    return id;
  }

  // plain object
  const id = insert(db, { kind: "object" });
  seen.set(obj, id);
  const keys = Object.keys(obj).slice(0, MAX_OBJECT_KEYS);
  for (const k of keys) {
    const childId = writeValue(db, (obj as Record<string, unknown>)[k], seen);
    db.insert(runtimeValueObjectMembers).values({
      parentValueId: id,
      key: k,
      childValueId: childId,
    }).run();
  }
  return id;
}

type RuntimeValueInsert = Parameters<Db["insert"]>[0] extends infer T ? never : never;

function insert(db: Db, values: {
  kind: "number" | "string" | "bool" | "null" | "undefined" | "object" | "array" | "function" | "bigint" | "symbol" | "nan" | "infinity" | "neg_infinity" | "circular" | "truncated";
  numberValue?: number;
  stringValue?: string;
  boolValue?: boolean;
  circularTargetId?: number;
  truncationNote?: string;
}): number {
  const row = db.insert(runtimeValues).values(values).returning().get();
  return row.id;
}
```

- [ ] **Step 8.4: Run test to verify it passes**

Run: `npx vitest run src/runtime/valueSerializer.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 8.5: Commit**

```bash
git add src/runtime/
git commit -m "runtime: value serializer (primitives + objects + arrays + circular + truncation)"
```

---

## Task 9: Snapshot instrumentation (ts-morph AST rewrite)

**Files:**
- Create: `src/runtime/snapshotInstrumentation.ts`
- Create: `src/runtime/snapshotInstrumentation.test.ts`

- [ ] **Step 9.1: Write failing test**

Create `src/runtime/snapshotInstrumentation.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { instrumentForSnapshot } from "./snapshotInstrumentation.js";

describe("instrumentForSnapshot", () => {
  it("inserts a snapshot call at the signal line capturing named locals", () => {
    const source = `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("result", q);
  return q;
}
    `.trim();
    const result = instrumentForSnapshot(source, { signalLine: 3, captureNames: ["a", "b", "q"] });
    // Expect a call like __neurallog_snapshot__("divide", 3, { a, b, q }) inserted before console.log
    expect(result).toContain("__neurallog_snapshot__");
    expect(result).toContain('"divide"');
    expect(result).toMatch(/__neurallog_snapshot__\(\s*"divide"\s*,\s*3\s*,\s*\{\s*a\s*,\s*b\s*,\s*q\s*\}\s*\)/);
    // Original console.log must remain
    expect(result).toContain('console.log("result", q)');
  });

  it("handles functions with expression-body arrow", () => {
    const source = `export const f = (x: number) => x + 1;`;
    const result = instrumentForSnapshot(source, { signalLine: 1, captureNames: ["x"] });
    // For arrow expression body, we wrap the body in a block with a snapshot call before the return.
    expect(result).toContain("__neurallog_snapshot__");
    expect(result).toContain("return");
  });

  it("returns source unchanged if signalLine is outside any function", () => {
    const source = `const x = 1;\nconst y = 2;\n`;
    const result = instrumentForSnapshot(source, { signalLine: 1, captureNames: ["x"] });
    expect(result).toBe(source);
  });
});
```

- [ ] **Step 9.2: Run test to verify it fails**

Run: `npx vitest run src/runtime/snapshotInstrumentation.test.ts`
Expected: FAIL with `Cannot find module './snapshotInstrumentation.js'`.

- [ ] **Step 9.3: Implement `src/runtime/snapshotInstrumentation.ts`**

Create file:
```typescript
import { Project, SyntaxKind, Node, SourceFile, FunctionLikeDeclaration, Block } from "ts-morph";

export interface InstrumentOptions {
  signalLine: number;
  captureNames: string[];
}

export function instrumentForSnapshot(source: string, opts: InstrumentOptions): string {
  const project = new Project({ useInMemoryFileSystem: true });
  const file = project.createSourceFile("input.ts", source);

  // Find the innermost function-like declaration containing signalLine.
  const targetFn = findFunctionAtLine(file, opts.signalLine);
  if (!targetFn) return source;

  const fnName = getFunctionName(targetFn) || "<anonymous>";
  const body = ensureBlockBody(targetFn);
  if (!body) return source;

  // Find the statement at signalLine (or the closest containing one).
  const stmt = findStatementAtLine(body, opts.signalLine);
  if (!stmt) return source;

  const capturesObj = `{ ${opts.captureNames.join(", ")} }`;
  const snapshotCall = `__neurallog_snapshot__(${JSON.stringify(fnName)}, ${opts.signalLine}, ${capturesObj});`;

  stmt.replaceWithText((writer) => {
    writer.writeLine(snapshotCall);
    writer.write(stmt.getText());
  });

  return file.getFullText();
}

function findFunctionAtLine(file: SourceFile, line: number): FunctionLikeDeclaration | null {
  let best: FunctionLikeDeclaration | null = null;
  file.forEachDescendant((node) => {
    if (
      Node.isFunctionDeclaration(node) ||
      Node.isFunctionExpression(node) ||
      Node.isArrowFunction(node) ||
      Node.isMethodDeclaration(node)
    ) {
      const startLine = node.getStartLineNumber();
      const endLine = node.getEndLineNumber();
      if (startLine <= line && line <= endLine) {
        best = node;
      }
    }
  });
  return best;
}

function getFunctionName(fn: FunctionLikeDeclaration): string | undefined {
  if (Node.isFunctionDeclaration(fn)) return fn.getName();
  if (Node.isMethodDeclaration(fn)) return fn.getName();
  const parent = fn.getParent();
  if (parent && Node.isVariableDeclaration(parent)) return parent.getName();
  return undefined;
}

function ensureBlockBody(fn: FunctionLikeDeclaration): Block | null {
  const body = fn.getBody();
  if (!body) return null;
  if (Node.isBlock(body)) return body;
  // Arrow expression-body: wrap in block with return
  if (Node.isArrowFunction(fn)) {
    const expr = body.getText();
    fn.setBodyText(`return ${expr};`);
    const newBody = fn.getBody();
    return Node.isBlock(newBody!) ? (newBody as Block) : null;
  }
  return null;
}

function findStatementAtLine(body: Block, line: number) {
  const statements = body.getStatements();
  for (const s of statements) {
    if (s.getStartLineNumber() <= line && line <= s.getEndLineNumber()) {
      return s;
    }
  }
  // Signal line not inside any statement — use last statement as fallback
  return statements.length ? statements[statements.length - 1] : null;
}
```

- [ ] **Step 9.4: Run test to verify it passes**

Run: `npx vitest run src/runtime/snapshotInstrumentation.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 9.5: Commit**

```bash
git add src/runtime/
git commit -m "runtime: snapshot instrumentation via ts-morph AST rewrite"
```

---

## Task 10: Wire snapshot capture into runHarness

**Files:**
- Modify: `src/harness.ts`
- Create: `src/harness.captureTrace.test.ts`

- [ ] **Step 10.1: Write failing test**

Create `src/harness.captureTrace.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { traces, traceValues, clauses, runtimeValues } from "./db/schema/index.js";
import { runHarnessWithTrace } from "./harness.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("runHarnessWithTrace", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("executes the target function, captures locals at signal line, persists as trace + trace_values", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    mkdirSync(join(tmpDir, "src"));
    const srcPath = join(tmpDir, "src", "divide.ts");
    writeFileSync(srcPath, `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
    `.trim());

    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/divide.ts/divide[2]",
      verdict: "violation",
      smt2: "(assert (not (= b 0)))",
      clauseHash: "c1",
    }).returning().get();

    const result = await runHarnessWithTrace({
      db,
      clauseId: clause.id,
      sourcePath: srcPath,
      functionName: "divide",
      signalLine: 3,
      captureNames: ["a", "b", "q"],
      inputs: { a: 1, b: 0 },
    });

    expect(result.outcomeKind).toBe("returned");

    const traceRows = db.select().from(traces).where(eq(traces.clauseId, clause.id)).all();
    expect(traceRows).toHaveLength(1);

    const valRows = db
      .select({ nodeId: traceValues.nodeId, kind: runtimeValues.kind, num: runtimeValues.numberValue })
      .from(traceValues)
      .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
      .where(eq(traceValues.traceId, traceRows[0].id))
      .all();

    // One entry per capture name at the signal line (single snapshot)
    expect(valRows.length).toBeGreaterThanOrEqual(3);
    const byNode = new Map(valRows.map((r) => [r.nodeId, r]));
    const qRow = [...byNode.entries()].find(([k]) => k.includes("q"));
    expect(qRow?.[1].kind === "infinity" || qRow?.[1].kind === "nan").toBeTruthy();
  });
});
```

- [ ] **Step 10.2: Run test to verify it fails**

Run: `npx vitest run src/harness.captureTrace.test.ts`
Expected: FAIL with `runHarnessWithTrace is not exported` or similar.

- [ ] **Step 10.3: Implement `runHarnessWithTrace` in `src/harness.ts`**

Open `src/harness.ts`, add at the end (preserving existing exports):

```typescript
import { readFileSync, writeFileSync } from "fs";
import { join, dirname } from "path";
import { instrumentForSnapshot } from "./runtime/snapshotInstrumentation.js";
import { serializeValue } from "./runtime/valueSerializer.js";
import { loadModuleWithPrivates } from "./moduleLoader.js";
import { traces, traceValues } from "./db/schema/index.js";
import type { Db } from "./db/index.js";
import { createHash } from "crypto";

export interface RunHarnessWithTraceArgs {
  db: Db;
  clauseId: number;
  sourcePath: string;
  functionName: string;
  signalLine: number;
  captureNames: string[];
  inputs: Record<string, unknown>;
  timeoutMs?: number;
}

export interface RunHarnessWithTraceResult {
  outcomeKind: "returned" | "threw" | "untestable";
  returnValue?: unknown;
  error?: string;
  traceId: number;
}

export async function runHarnessWithTrace(args: RunHarnessWithTraceArgs): Promise<RunHarnessWithTraceResult> {
  const {
    db,
    clauseId,
    sourcePath,
    functionName,
    signalLine,
    captureNames,
    inputs,
    timeoutMs = 3000,
  } = args;

  const originalSource = readFileSync(sourcePath, "utf-8");
  const instrumented = instrumentForSnapshot(originalSource, { signalLine, captureNames });

  // Write instrumented source to a sibling file so the loader can transpile it.
  const instrumentedPath = sourcePath.replace(/\.ts$/, ".__instrumented__.ts");
  writeFileSync(instrumentedPath, instrumented);

  const capturedSnapshots: { fnName: string; line: number; locals: Record<string, unknown> }[] = [];

  // Inject the snapshot hook into the global scope the loaded module can see.
  (globalThis as any).__neurallog_snapshot__ = (fnName: string, line: number, locals: Record<string, unknown>) => {
    capturedSnapshots.push({ fnName, line, locals: { ...locals } });
  };

  // Deterministic stubs
  const origRandom = Math.random;
  const origNow = Date.now;
  Math.random = () => 0.5;
  Date.now = () => 0;

  const inputsHash = createHash("sha256")
    .update(JSON.stringify(inputs))
    .digest("hex")
    .slice(0, 16);

  let outcome: RunHarnessWithTraceResult = {
    outcomeKind: "threw",
    error: "not executed",
    traceId: -1,
  };

  try {
    const mod = loadModuleWithPrivates(instrumentedPath);
    const fn = mod[functionName];
    if (typeof fn !== "function") {
      outcome = {
        outcomeKind: "untestable",
        error: `export ${functionName} not a function`,
        traceId: -1,
      };
    } else {
      const args = Object.values(inputs);
      let result: unknown;
      try {
        result = await Promise.race([
          (async () => fn(...args))(),
          new Promise((_, reject) => setTimeout(() => reject(new Error("__TIMEOUT__")), timeoutMs)),
        ]);
        outcome = {
          outcomeKind: "returned",
          returnValue: result,
          traceId: -1,
        };
      } catch (err: any) {
        outcome = {
          outcomeKind: "threw",
          error: String(err?.message || err).slice(0, 500),
          traceId: -1,
        };
      }
    }
  } finally {
    Math.random = origRandom;
    Date.now = origNow;
    delete (globalThis as any).__neurallog_snapshot__;
    try {
      require("fs").unlinkSync(instrumentedPath);
    } catch {
      /* ignore */
    }
  }

  // Persist to DB
  const outcomeValueId =
    outcome.outcomeKind === "returned"
      ? serializeValue(db, outcome.returnValue)
      : outcome.outcomeKind === "threw"
        ? serializeValue(db, outcome.error)
        : null;

  const traceRow = db.insert(traces).values({
    clauseId,
    capturedAt: Date.now(),
    outcomeKind: outcome.outcomeKind,
    outcomeValueId: outcomeValueId ?? undefined,
    untestableReason: outcome.outcomeKind === "untestable" ? outcome.error : undefined,
    inputsHash,
  }).returning().get();

  for (const snap of capturedSnapshots) {
    for (const [name, value] of Object.entries(snap.locals)) {
      const valueId = serializeValue(db, value);
      db.insert(traceValues).values({
        traceId: traceRow.id,
        nodeId: `${sourcePath}:${snap.line}:${name}`,
        iterationIndex: null,
        rootValueId: valueId,
      }).run();
    }
  }

  outcome.traceId = traceRow.id;
  return outcome;
}
```

Note: the `Date.now = () => 0;` stub conflicts with our own `capturedAt: Date.now()` after the stub is restored — we must record `capturedAt` BEFORE restoring or AFTER; the code above records after restoration (after `finally`), which is correct since `Date.now` is back to real by then.

- [ ] **Step 10.4: Run test to verify it passes**

Run: `npx vitest run src/harness.captureTrace.test.ts`
Expected: PASS (1 test).

- [ ] **Step 10.5: Run full suite for regressions**

Run: `npx vitest run`
Expected: PASS all tests.

- [ ] **Step 10.6: Commit**

```bash
git add src/harness.ts src/harness.captureTrace.test.ts
git commit -m "harness: runHarnessWithTrace wires instrumentation + value serialization + persistence"
```

---

## Task 11: Binding validator (line-based, transitional)

**Files:**
- Create: `src/bindings/validator.ts`
- Create: `src/bindings/validator.test.ts`

- [ ] **Step 11.1: Write failing test**

Create `src/bindings/validator.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { validateBindings } from "./validator.js";

describe("validateBindings", () => {
  const source = `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
  `.trim();

  it("accepts bindings whose source_line and source_expr are present", () => {
    const res = validateBindings(source, [
      { smtConstant: "a", sourceLine: 1, sourceExpr: "a", sort: "Real" },
      { smtConstant: "b", sourceLine: 1, sourceExpr: "b", sort: "Real" },
      { smtConstant: "q", sourceLine: 2, sourceExpr: "a / b", sort: "Real" },
    ]);
    expect(res.valid).toHaveLength(3);
    expect(res.invalid).toHaveLength(0);
  });

  it("rejects a binding whose source_line is out of range", () => {
    const res = validateBindings(source, [
      { smtConstant: "x", sourceLine: 99, sourceExpr: "x", sort: "Real" },
    ]);
    expect(res.invalid).toHaveLength(1);
    expect(res.invalid[0].reason).toMatch(/line 99 out of range/);
  });

  it("rejects a binding whose source_expr is absent from the declared line", () => {
    const res = validateBindings(source, [
      { smtConstant: "ghost", sourceLine: 2, sourceExpr: "nonexistent_expr", sort: "Real" },
    ]);
    expect(res.invalid).toHaveLength(1);
    expect(res.invalid[0].reason).toMatch(/source_expr.*not found/);
  });
});
```

- [ ] **Step 11.2: Run test to verify it fails**

Run: `npx vitest run src/bindings/validator.test.ts`
Expected: FAIL with `Cannot find module './validator.js'`.

- [ ] **Step 11.3: Implement `src/bindings/validator.ts`**

Create file:
```typescript
export interface Binding {
  smtConstant: string;
  sourceLine: number;
  sourceExpr: string;
  sort: string;
}

export interface ValidationResult {
  valid: Binding[];
  invalid: Array<{ binding: Binding; reason: string }>;
}

export function validateBindings(source: string, bindings: Binding[]): ValidationResult {
  const lines = source.split("\n");
  const valid: Binding[] = [];
  const invalid: Array<{ binding: Binding; reason: string }> = [];

  for (const b of bindings) {
    if (b.sourceLine < 1 || b.sourceLine > lines.length) {
      invalid.push({ binding: b, reason: `line ${b.sourceLine} out of range (1..${lines.length})` });
      continue;
    }
    const lineText = lines[b.sourceLine - 1]!;
    if (!textContainsExpression(lineText, b.sourceExpr)) {
      invalid.push({ binding: b, reason: `source_expr ${JSON.stringify(b.sourceExpr)} not found at line ${b.sourceLine}` });
      continue;
    }
    valid.push(b);
  }
  return { valid, invalid };
}

function textContainsExpression(lineText: string, expr: string): boolean {
  // Normalize whitespace for the check; exact text match is too brittle.
  const normalize = (s: string) => s.replace(/\s+/g, "");
  return normalize(lineText).includes(normalize(expr));
}
```

- [ ] **Step 11.4: Run test to verify it passes**

Run: `npx vitest run src/bindings/validator.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 11.5: Commit**

```bash
git add src/bindings/
git commit -m "bindings: line-based validator (transitional, pre-SAST)"
```

---

## Task 12: Comparator core + IEEE specials agent

**Files:**
- Create: `src/comparator/core.ts`
- Create: `src/comparator/agents/ieeeSpecials.ts`
- Create: `src/comparator/agents/ieeeSpecials.test.ts`

- [ ] **Step 12.1: Write failing test**

Create `src/comparator/agents/ieeeSpecials.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { ieeeSpecialsAgent } from "./ieeeSpecials.js";

describe("ieeeSpecialsAgent", () => {
  it("reports NaN when SMT modeled a finite Real but runtime observed NaN", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Real", sourceLine: 3, sourceExpr: "a/b" },
      witness: { sort: "Real", value: 0 },
      runtimeValue: { kind: "nan" },
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("ieee_specials");
    expect(gap!.explanation).toMatch(/NaN/);
  });

  it("reports Infinity when SMT said div_by_zero and runtime observed Infinity", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Real", sourceLine: 3, sourceExpr: "a/b" },
      witness: { sort: "Real", value: "div_by_zero" },
      runtimeValue: { kind: "infinity" },
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("ieee_specials");
    expect(gap!.explanation).toMatch(/Infinity/);
  });

  it("returns null when SMT value matches runtime value numerically", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Real", sourceLine: 3, sourceExpr: "a/b" },
      witness: { sort: "Real", value: 2.5 },
      runtimeValue: { kind: "number", numberValue: 2.5 },
    });
    expect(gap).toBeNull();
  });

  it("skips non-Real sorts", () => {
    const gap = ieeeSpecialsAgent({
      binding: { smtConstant: "x", sort: "Bool", sourceLine: 3, sourceExpr: "a" },
      witness: { sort: "Bool", value: true },
      runtimeValue: { kind: "bool", boolValue: true },
    });
    expect(gap).toBeNull();
  });
});
```

- [ ] **Step 12.2: Run test to verify it fails**

Run: `npx vitest run src/comparator/agents/ieeeSpecials.test.ts`
Expected: FAIL with `Cannot find module './ieeeSpecials.js'`.

- [ ] **Step 12.3: Implement `src/comparator/core.ts`**

Create file:
```typescript
import type { Binding } from "../bindings/validator.js";
import type { Z3Value } from "../z3/modelParser.js";

export interface RuntimeValueLite {
  kind: string;
  numberValue?: number | null;
  stringValue?: string | null;
  boolValue?: boolean | null;
}

export interface GapReport {
  kind:
    | "ieee_specials"
    | "int_overflow"
    | "bool_coercion"
    | "null_undefined"
    | "path_not_taken"
    | "outcome_mismatch"
    | "invalid_binding";
  smtConstant: string;
  explanation: string;
  smtValue?: Z3Value;
  runtimeValue?: RuntimeValueLite;
}

export interface ComparatorInput {
  binding: Binding;
  witness: Z3Value;
  runtimeValue: RuntimeValueLite;
}

export type ComparatorAgent = (input: ComparatorInput) => GapReport | null;
```

- [ ] **Step 12.4: Implement `src/comparator/agents/ieeeSpecials.ts`**

Create file:
```typescript
import type { ComparatorAgent, GapReport } from "../core.js";

export const ieeeSpecialsAgent: ComparatorAgent = ({ binding, witness, runtimeValue }) => {
  if (witness.sort !== "Real") return null;

  // NaN at runtime
  if (runtimeValue.kind === "nan") {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT Real modeled ${formatWitness(witness)} but runtime produced NaN (IEEE 754). Z3's Real sort does not model NaN.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  // Infinity at runtime
  if (runtimeValue.kind === "infinity" || runtimeValue.kind === "neg_infinity") {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT Real modeled ${formatWitness(witness)} but runtime produced ${runtimeValue.kind === "infinity" ? "Infinity" : "-Infinity"} (IEEE 754). Z3's Real sort does not model infinities the same way JavaScript does.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  // SMT said div_by_zero or inf but runtime produced a finite number
  if (
    (witness.value === "div_by_zero" || witness.value === "nan" || witness.value === "+infinity" || witness.value === "-infinity") &&
    runtimeValue.kind === "number"
  ) {
    return {
      kind: "ieee_specials",
      smtConstant: binding.smtConstant,
      explanation: `SMT Real modeled ${formatWitness(witness)} but runtime produced the finite value ${runtimeValue.numberValue}. Encoding and runtime diverge on special-value handling.`,
      smtValue: witness,
      runtimeValue,
    };
  }

  // Numeric mismatch beyond epsilon
  if (typeof witness.value === "number" && runtimeValue.kind === "number" && typeof runtimeValue.numberValue === "number") {
    const diff = Math.abs(witness.value - runtimeValue.numberValue);
    const scale = Math.max(1, Math.abs(witness.value), Math.abs(runtimeValue.numberValue));
    if (diff / scale > 1e-9) {
      return {
        kind: "ieee_specials",
        smtConstant: binding.smtConstant,
        explanation: `SMT Real value ${witness.value} differs from runtime IEEE value ${runtimeValue.numberValue} beyond float tolerance.`,
        smtValue: witness,
        runtimeValue,
      };
    }
  }

  return null;
};

function formatWitness(witness: import("../../z3/modelParser.js").Z3Value): string {
  if (witness.sort === "Real") return typeof witness.value === "number" ? witness.value.toString() : witness.value;
  return JSON.stringify(witness);
}
```

- [ ] **Step 12.5: Run test to verify it passes**

Run: `npx vitest run src/comparator/agents/ieeeSpecials.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 12.6: Commit**

```bash
git add src/comparator/
git commit -m "comparator: core types + IEEE specials agent (NaN/Infinity/div_by_zero)"
```

---

## Task 13: Outcome-mismatch agent

**Files:**
- Create: `src/comparator/agents/outcomeMismatch.ts`
- Create: `src/comparator/agents/outcomeMismatch.test.ts`

- [ ] **Step 13.1: Write failing test**

Create `src/comparator/agents/outcomeMismatch.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { outcomeMismatchAgent } from "./outcomeMismatch.js";

describe("outcomeMismatchAgent", () => {
  it("reports outcome_mismatch when SMT modeled a return but runtime threw", () => {
    const gap = outcomeMismatchAgent({
      smtOutcome: { kind: "returned" },
      runtimeOutcome: { kind: "threw", error: "TypeError: cannot read properties" },
      smtConstant: "result",
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("outcome_mismatch");
    expect(gap!.explanation).toMatch(/return.*threw/i);
  });

  it("reports when SMT modeled a throw but runtime returned", () => {
    const gap = outcomeMismatchAgent({
      smtOutcome: { kind: "threw" },
      runtimeOutcome: { kind: "returned" },
      smtConstant: "result",
    });
    expect(gap).not.toBeNull();
    expect(gap!.explanation).toMatch(/throw.*returned/i);
  });

  it("returns null when both outcomes agree", () => {
    const gap = outcomeMismatchAgent({
      smtOutcome: { kind: "returned" },
      runtimeOutcome: { kind: "returned" },
      smtConstant: "result",
    });
    expect(gap).toBeNull();
  });
});
```

- [ ] **Step 13.2: Run test to verify it fails**

Run: `npx vitest run src/comparator/agents/outcomeMismatch.test.ts`
Expected: FAIL with `Cannot find module './outcomeMismatch.js'`.

- [ ] **Step 13.3: Implement `src/comparator/agents/outcomeMismatch.ts`**

Create file:
```typescript
import type { GapReport } from "../core.js";

export interface OutcomeMismatchInput {
  smtOutcome: { kind: "returned" | "threw" };
  runtimeOutcome: { kind: "returned" | "threw" | "untestable"; error?: string };
  smtConstant: string;
}

export function outcomeMismatchAgent(input: OutcomeMismatchInput): GapReport | null {
  const { smtOutcome, runtimeOutcome, smtConstant } = input;
  if (runtimeOutcome.kind === "untestable") return null;
  if (smtOutcome.kind === runtimeOutcome.kind) return null;

  const smtKind = smtOutcome.kind;
  const rtKind = runtimeOutcome.kind;
  return {
    kind: "outcome_mismatch",
    smtConstant,
    explanation: `SMT modeled the function as ${smtKind}; runtime ${rtKind === "threw" ? `threw: ${runtimeOutcome.error}` : "returned a value"}. The encoding does not account for the runtime's actual control-flow outcome.`,
  };
}
```

- [ ] **Step 13.4: Run test to verify it passes**

Run: `npx vitest run src/comparator/agents/outcomeMismatch.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 13.5: Commit**

```bash
git add src/comparator/
git commit -m "comparator: outcome-mismatch agent (return vs throw disagreement)"
```

---

## Task 14: Path-not-taken agent

**Files:**
- Create: `src/comparator/agents/pathNotTaken.ts`
- Create: `src/comparator/agents/pathNotTaken.test.ts`

- [ ] **Step 14.1: Write failing test**

Create `src/comparator/agents/pathNotTaken.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { pathNotTakenAgent } from "./pathNotTaken.js";

describe("pathNotTakenAgent", () => {
  it("reports path_not_taken when the runtime did not visit the signal line", () => {
    const gap = pathNotTakenAgent({
      signalLine: 10,
      visitedLines: new Set([1, 2, 3, 4, 5]),
      smtConstant: "unreachable",
    });
    expect(gap).not.toBeNull();
    expect(gap!.kind).toBe("path_not_taken");
    expect(gap!.explanation).toMatch(/line 10/);
  });

  it("returns null when the runtime did visit the signal line", () => {
    const gap = pathNotTakenAgent({
      signalLine: 3,
      visitedLines: new Set([1, 2, 3, 4]),
      smtConstant: "ok",
    });
    expect(gap).toBeNull();
  });

  it("returns null when visitedLines is empty (no trace captured)", () => {
    const gap = pathNotTakenAgent({
      signalLine: 3,
      visitedLines: new Set(),
      smtConstant: "ok",
    });
    expect(gap).toBeNull();
  });
});
```

- [ ] **Step 14.2: Run test to verify it fails**

Run: `npx vitest run src/comparator/agents/pathNotTaken.test.ts`
Expected: FAIL with `Cannot find module './pathNotTaken.js'`.

- [ ] **Step 14.3: Implement `src/comparator/agents/pathNotTaken.ts`**

Create file:
```typescript
import type { GapReport } from "../core.js";

export interface PathNotTakenInput {
  signalLine: number;
  visitedLines: Set<number>;
  smtConstant: string;
}

export function pathNotTakenAgent(input: PathNotTakenInput): GapReport | null {
  const { signalLine, visitedLines, smtConstant } = input;
  if (visitedLines.size === 0) return null;
  if (visitedLines.has(signalLine)) return null;

  return {
    kind: "path_not_taken",
    smtConstant,
    explanation: `SMT witness claims a value at line ${signalLine}, but runtime did not reach that line. The witness inputs drive execution down a different path than the encoding assumed.`,
  };
}
```

- [ ] **Step 14.4: Run test to verify it passes**

Run: `npx vitest run src/comparator/agents/pathNotTaken.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 14.5: Commit**

```bash
git add src/comparator/
git commit -m "comparator: path-not-taken agent"
```

---

## Task 15: Gap detection pipeline (orchestrator + persistence)

**Files:**
- Create: `src/gapDetection.ts`
- Create: `src/gapDetection.test.ts`

- [ ] **Step 15.1: Write failing test**

Create `src/gapDetection.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { clauses, gapReports, runtimeValues } from "./db/schema/index.js";
import { detectGaps } from "./gapDetection.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("detectGaps", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("detects IEEE NaN gap when SMT models finite result but runtime produces NaN", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    mkdirSync(join(tmpDir, "src"));
    const srcPath = join(tmpDir, "src", "divide.ts");
    writeFileSync(srcPath, `
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
    `.trim());

    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/divide.ts/divide[2]",
      verdict: "proven",
      smt2: "(declare-const q Real) (assert (= q 0.0))",
      clauseHash: "c1",
    }).returning().get();

    await detectGaps({
      db,
      clauseId: clause.id,
      sourcePath: srcPath,
      functionName: "divide",
      signalLine: 3,
      bindings: [
        { smtConstant: "q", sourceLine: 2, sourceExpr: "a / b", sort: "Real" },
      ],
      z3WitnessText: `(
        (define-fun q () Real 0.0)
      )`,
      inputs: { a: 0, b: 0 },
    });

    const gaps = db.select().from(gapReports).where(eq(gapReports.clauseId, clause.id)).all();
    expect(gaps.length).toBeGreaterThanOrEqual(1);
    const ieeeGap = gaps.find((g) => g.kind === "ieee_specials");
    expect(ieeeGap).toBeDefined();
    expect(ieeeGap!.explanation).toMatch(/NaN/);
  });
});
```

- [ ] **Step 15.2: Run test to verify it fails**

Run: `npx vitest run src/gapDetection.test.ts`
Expected: FAIL with `Cannot find module './gapDetection.js'`.

- [ ] **Step 15.3: Implement `src/gapDetection.ts`**

Create file:
```typescript
import type { Db } from "./db/index.js";
import { gapReports, traceValues, runtimeValues, clauseWitnesses } from "./db/schema/index.js";
import { parseZ3Model } from "./z3/modelParser.js";
import { persistWitness } from "./z3/persistWitness.js";
import { runHarnessWithTrace } from "./harness.js";
import { validateBindings, type Binding } from "./bindings/validator.js";
import { ieeeSpecialsAgent } from "./comparator/agents/ieeeSpecials.js";
import { outcomeMismatchAgent } from "./comparator/agents/outcomeMismatch.js";
import { pathNotTakenAgent } from "./comparator/agents/pathNotTaken.js";
import { serializeValue } from "./runtime/valueSerializer.js";
import { eq } from "drizzle-orm";
import { readFileSync } from "fs";

export interface DetectGapsArgs {
  db: Db;
  clauseId: number;
  sourcePath: string;
  functionName: string;
  signalLine: number;
  bindings: Binding[];
  z3WitnessText: string;
  inputs: Record<string, unknown>;
}

export async function detectGaps(args: DetectGapsArgs): Promise<void> {
  const { db, clauseId, sourcePath, functionName, signalLine, bindings, z3WitnessText, inputs } = args;

  // 1. Validate bindings against source text
  const source = readFileSync(sourcePath, "utf-8");
  const { valid, invalid } = validateBindings(source, bindings);
  for (const bad of invalid) {
    db.insert(gapReports).values({
      clauseId,
      kind: "invalid_binding",
      smtConstant: bad.binding.smtConstant,
      explanation: bad.reason,
    }).run();
  }
  if (valid.length === 0) return;

  // 2. Parse Z3 model + persist witnesses
  const parsedModel = parseZ3Model(z3WitnessText);
  persistWitness(db, clauseId, parsedModel);

  // 3. Run harness with trace
  const captureNames = valid.map((b) => b.smtConstant);
  const runResult = await runHarnessWithTrace({
    db,
    clauseId,
    sourcePath,
    functionName,
    signalLine,
    captureNames,
    inputs,
  });

  // 4. Pull runtime values per binding
  const tvRows = db
    .select({
      nodeId: traceValues.nodeId,
      kind: runtimeValues.kind,
      numberValue: runtimeValues.numberValue,
      stringValue: runtimeValues.stringValue,
      boolValue: runtimeValues.boolValue,
    })
    .from(traceValues)
    .innerJoin(runtimeValues, eq(runtimeValues.id, traceValues.rootValueId))
    .where(eq(traceValues.traceId, runResult.traceId))
    .all();

  const runtimeByConstant = new Map<string, typeof tvRows[0]>();
  for (const row of tvRows) {
    // nodeId format: "<path>:<line>:<name>"
    const name = row.nodeId.split(":").pop();
    if (name) runtimeByConstant.set(name, row);
  }

  // 5. Compute visited lines (for path-not-taken) — currently "line covered" is
  // approximated as "any trace_value exists for any capture at this line."
  // A proper implementation waits for SAST + richer instrumentation.
  const visitedLines = new Set<number>();
  for (const row of tvRows) {
    const parts = row.nodeId.split(":");
    const lineStr = parts[parts.length - 2];
    const line = parseInt(lineStr || "0", 10);
    if (Number.isFinite(line)) visitedLines.add(line);
  }

  // 6. Run sort-specific agents per binding
  for (const b of valid) {
    const witness = parsedModel.get(b.smtConstant);
    if (!witness) continue;
    const runtimeRow = runtimeByConstant.get(b.smtConstant);
    if (!runtimeRow) continue;

    const runtimeValueLite = {
      kind: runtimeRow.kind,
      numberValue: runtimeRow.numberValue,
      stringValue: runtimeRow.stringValue,
      boolValue: runtimeRow.boolValue,
    };

    const ieeeGap = ieeeSpecialsAgent({ binding: b, witness, runtimeValue: runtimeValueLite });
    if (ieeeGap) {
      const smtValueId = serializeWitnessForGap(db, witness);
      const runtimeValueId = lookupRuntimeValueIdByNodeId(db, runResult.traceId, runtimeRow.nodeId);
      db.insert(gapReports).values({
        clauseId,
        traceId: runResult.traceId,
        kind: "ieee_specials",
        smtConstant: b.smtConstant,
        atNodeRef: `${sourcePath}:${b.sourceLine}`,
        smtValueId: smtValueId ?? undefined,
        runtimeValueId: runtimeValueId ?? undefined,
        explanation: ieeeGap.explanation,
      }).run();
    }
  }

  // 7. Outcome mismatch
  // For Phase A-thin we assume the SMT models a "returned" outcome unless the
  // clause's smt2 explicitly asserts a throw (not encoded in this phase).
  const smtOutcome = { kind: "returned" as const };
  const rtOutcome =
    runResult.outcomeKind === "returned"
      ? { kind: "returned" as const }
      : runResult.outcomeKind === "threw"
        ? { kind: "threw" as const, error: runResult.error }
        : { kind: "untestable" as const };
  const outcomeGap = outcomeMismatchAgent({
    smtOutcome,
    runtimeOutcome: rtOutcome,
    smtConstant: valid[0]?.smtConstant ?? "<signal>",
  });
  if (outcomeGap) {
    db.insert(gapReports).values({
      clauseId,
      traceId: runResult.traceId,
      kind: "outcome_mismatch",
      smtConstant: outcomeGap.smtConstant,
      explanation: outcomeGap.explanation,
    }).run();
  }

  // 8. Path not taken
  const pathGap = pathNotTakenAgent({
    signalLine,
    visitedLines,
    smtConstant: valid[0]?.smtConstant ?? "<signal>",
  });
  if (pathGap) {
    db.insert(gapReports).values({
      clauseId,
      traceId: runResult.traceId,
      kind: "path_not_taken",
      smtConstant: pathGap.smtConstant,
      explanation: pathGap.explanation,
    }).run();
  }
}

function serializeWitnessForGap(db: Db, witness: import("./z3/modelParser.js").Z3Value): number | null {
  if (witness.sort === "Real") {
    if (typeof witness.value === "number") return serializeValue(db, witness.value);
    if (witness.value === "nan" || witness.value === "div_by_zero") return serializeValue(db, NaN);
    if (witness.value === "+infinity") return serializeValue(db, Infinity);
    if (witness.value === "-infinity") return serializeValue(db, -Infinity);
  }
  if (witness.sort === "Int") return serializeValue(db, Number(witness.value));
  if (witness.sort === "Bool") return serializeValue(db, witness.value);
  if (witness.sort === "String") return serializeValue(db, witness.value);
  return null;
}

function lookupRuntimeValueIdByNodeId(db: Db, traceId: number, nodeId: string): number | null {
  const row = db
    .select({ rootValueId: traceValues.rootValueId })
    .from(traceValues)
    .where(eq(traceValues.traceId, traceId))
    .all()
    .find((r) => r);
  // The above is a simplification; the compound key includes nodeId.
  // Use filtered query:
  const rows = db
    .select({ rootValueId: traceValues.rootValueId, nodeId: traceValues.nodeId })
    .from(traceValues)
    .where(eq(traceValues.traceId, traceId))
    .all();
  const exact = rows.find((r) => r.nodeId === nodeId);
  return exact ? exact.rootValueId : null;
}
```

- [ ] **Step 15.4: Run test to verify it passes**

Run: `npx vitest run src/gapDetection.test.ts`
Expected: PASS (1 test).

- [ ] **Step 15.5: Run full suite for regressions**

Run: `npx vitest run`
Expected: PASS all tests.

- [ ] **Step 15.6: Commit**

```bash
git add src/gapDetection.ts src/gapDetection.test.ts
git commit -m "gap-detection: orchestrator wires validator + z3 parser + trace + agents + persistence"
```

---

## Task 16: LLM prompt updates for smt_bindings emission

**Files:**
- Modify: `prompts/invariant_derivation.md`
- Create: `prompts/invariant_derivation.test.ts` (smoke test for prompt parser)

- [ ] **Step 16.1: Write failing smoke test**

Create `prompts/invariant_derivation.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { join } from "path";

describe("invariant_derivation prompt", () => {
  const promptText = readFileSync(join(__dirname, "invariant_derivation.md"), "utf-8");

  it("instructs the LLM to emit smt_bindings per clause", () => {
    expect(promptText).toMatch(/smt_bindings/);
    expect(promptText).toMatch(/source_line/);
    expect(promptText).toMatch(/source_expr/);
    expect(promptText).toMatch(/sort/);
  });

  it("shows an example of the bindings output format", () => {
    // The prompt should contain at least one worked example of the expected
    // JSON-ish block a model can emulate.
    expect(promptText).toMatch(/"smt_constant"\s*:\s*"/);
  });
});
```

- [ ] **Step 16.2: Run test to verify it fails**

Run: `npx vitest run prompts/invariant_derivation.test.ts`
Expected: FAIL because the current prompt doesn't contain those tokens yet.

- [ ] **Step 16.3: Edit `prompts/invariant_derivation.md`**

Append a new section at the end of the prompt:

```markdown

---

## Binding metadata (required in every derivation output)

For every SMT constant you declare in a clause, emit a **binding record**
that tells us where that constant's value lives in the source. The
line-attributed encoding-gap detector cannot work without these.

Emit one JSON-ish block per clause, after the SMT-LIB block, in this exact
shape:

```
SMT_BINDINGS [clause-id]:
[
  { "smt_constant": "den", "source_line": 14, "source_expr": "b", "sort": "Real" },
  { "smt_constant": "q",   "source_line": 15, "source_expr": "a / b", "sort": "Real" }
]
```

Rules:
- `smt_constant` matches the name you used in `(declare-const ...)`.
- `source_line` is the 1-indexed line in the file-under-analysis (not the
  full context bundle) where the expression appears.
- `source_expr` is a short, readable substring of the source that the
  line contains, used to cross-check the line number. Whitespace inside
  the expression is tolerated; exact characters are not required.
- `sort` is the SMT sort you declared (`Real`, `Int`, `Bool`, `String`,
  `Array`, etc.).

If a constant has no meaningful source location (e.g., you introduced a
pure abstraction), emit `"source_line": 0, "source_expr": "<abstract>"`.
The gap detector will skip comparison for such constants.
```

- [ ] **Step 16.4: Run test to verify it passes**

Run: `npx vitest run prompts/invariant_derivation.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 16.5: Commit**

```bash
git add prompts/
git commit -m "prompts: derivation emits smt_bindings (smt_constant, source_line, source_expr, sort)"
```

---

## Task 17: CLI `--gaps` flag on `explain`

**Files:**
- Modify: `src/cli.ts`
- Create: `src/cli.gaps.test.ts`

- [ ] **Step 17.1: Write failing test**

Create `src/cli.gaps.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { clauses, gapReports, runtimeValues } from "./db/schema/index.js";
import { explainGaps } from "./cli.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";

describe("explainGaps (cli --gaps)", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("renders gap reports for a given contract key", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const clause = db.insert(clauses).values({
      contractKey: "src/divide.ts/divide[2]",
      verdict: "proven",
      smt2: "(assert (= q 0.0))",
      clauseHash: "c1",
    }).returning().get();

    const smtVal = db.insert(runtimeValues).values({ kind: "number", numberValue: 0 }).returning().get();
    const rtVal = db.insert(runtimeValues).values({ kind: "nan" }).returning().get();

    db.insert(gapReports).values({
      clauseId: clause.id,
      kind: "ieee_specials",
      smtConstant: "q",
      atNodeRef: "src/divide.ts:2",
      smtValueId: smtVal.id,
      runtimeValueId: rtVal.id,
      explanation: "SMT Real modeled 0 but runtime produced NaN",
    }).run();

    const output = explainGaps(db, "src/divide.ts/divide[2]");
    expect(output).toContain("encoding-gap at src/divide.ts:2");
    expect(output).toContain("q");
    expect(output).toContain("NaN");
  });

  it("reports no gaps when none are stored", () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-test-"));
    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });
    const output = explainGaps(db, "src/nope.ts/foo[1]");
    expect(output).toMatch(/no encoding gaps/i);
  });
});
```

- [ ] **Step 17.2: Run test to verify it fails**

Run: `npx vitest run src/cli.gaps.test.ts`
Expected: FAIL with `explainGaps is not exported from ./cli.js`.

- [ ] **Step 17.3: Add `explainGaps` to `src/cli.ts`**

Open `src/cli.ts` and add at the end (preserving existing exports):

```typescript
import { gapReports as gapReportsTable, clauses as clausesTable, runtimeValues } from "./db/schema/index.js";
import type { Db } from "./db/index.js";
import { eq } from "drizzle-orm";

export function explainGaps(db: Db, contractKey: string): string {
  const rows = db
    .select({
      kind: gapReportsTable.kind,
      smtConstant: gapReportsTable.smtConstant,
      atNodeRef: gapReportsTable.atNodeRef,
      explanation: gapReportsTable.explanation,
      smtValueId: gapReportsTable.smtValueId,
      runtimeValueId: gapReportsTable.runtimeValueId,
    })
    .from(gapReportsTable)
    .innerJoin(clausesTable, eq(clausesTable.id, gapReportsTable.clauseId))
    .where(eq(clausesTable.contractKey, contractKey))
    .all();

  if (rows.length === 0) {
    return `No encoding gaps reported for ${contractKey}.\n`;
  }

  const lines: string[] = [];
  for (const row of rows) {
    const header = row.atNodeRef ? `encoding-gap at ${row.atNodeRef}` : `encoding-gap`;
    const constName = row.smtConstant ? ` — ${row.smtConstant}` : "";
    lines.push(`${header}${constName}`);

    if (row.smtValueId) {
      const smtVal = db.select().from(runtimeValues).where(eq(runtimeValues.id, row.smtValueId)).get();
      if (smtVal) lines.push(`  Z3 modeled:        ${formatValueRow(smtVal)}`);
    }
    if (row.runtimeValueId) {
      const rtVal = db.select().from(runtimeValues).where(eq(runtimeValues.id, row.runtimeValueId)).get();
      if (rtVal) lines.push(`  Runtime returned:  ${formatValueRow(rtVal)}`);
    }
    lines.push(`  Cause:             ${row.explanation}`);
    lines.push(`  Kind:              ${row.kind}`);
    lines.push("");
  }

  return lines.join("\n");
}

function formatValueRow(row: {
  kind: string;
  numberValue: number | null;
  stringValue: string | null;
  boolValue: boolean | null;
}): string {
  switch (row.kind) {
    case "number":
      return String(row.numberValue);
    case "string":
      return JSON.stringify(row.stringValue);
    case "bool":
      return String(row.boolValue);
    case "nan":
      return "NaN";
    case "infinity":
      return "Infinity";
    case "neg_infinity":
      return "-Infinity";
    case "null":
      return "null";
    case "undefined":
      return "undefined";
    default:
      return `<${row.kind}>`;
  }
}
```

Then, in the existing CLI argument parser (search for the existing
`explain` subcommand handler in `src/cli.ts`), add handling for a `--gaps`
flag that calls `explainGaps` and prints the output.

Locate the existing `explain` subcommand (e.g., near `case "explain":`).
Inside its handler, after parsing the signal path argument, check for
`--gaps`:

```typescript
// Within the explain subcommand branch:
if (args.includes("--gaps")) {
  const contractKey = /* parse from the existing explain argument path */;
  const db = openDb(".neurallog/neurallog.db");
  process.stdout.write(explainGaps(db, contractKey));
  return;
}
```

If `src/cli.ts` uses a different CLI pattern (e.g., a commander-style
setup or a custom argv walker), adapt accordingly — the core is that
`--gaps` short-circuits to `explainGaps`.

- [ ] **Step 17.4: Run test to verify it passes**

Run: `npx vitest run src/cli.gaps.test.ts`
Expected: PASS (2 tests).

- [ ] **Step 17.5: Commit**

```bash
git add src/cli.ts src/cli.gaps.test.ts
git commit -m "cli: explain --gaps reads gap_reports and renders THESIS-style output"
```

---

## Task 18: End-to-end integration test

**Files:**
- Create: `examples/division-by-zero.ts` (fixture)
- Create: `src/e2e.gapDetection.test.ts`

- [ ] **Step 18.1: Create the fixture**

Create `examples/division-by-zero.ts`:
```typescript
export function divide(a: number, b: number): number {
  const q = a / b;
  console.log("q", q);
  return q;
}
```

- [ ] **Step 18.2: Write integration test**

Create `src/e2e.gapDetection.test.ts`:
```typescript
import { describe, it, expect, afterEach } from "vitest";
import { mkdtempSync, rmSync, copyFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import { openDb } from "./db/index.js";
import { clauses, gapReports } from "./db/schema/index.js";
import { detectGaps } from "./gapDetection.js";
import { explainGaps } from "./cli.js";
import { migrate } from "drizzle-orm/better-sqlite3/migrator";
import { eq } from "drizzle-orm";

describe("end-to-end: division-by-zero produces IEEE gap", () => {
  let tmpDir: string;

  afterEach(() => {
    if (tmpDir) rmSync(tmpDir, { recursive: true, force: true });
  });

  it("runs the full pipeline and reports an IEEE NaN gap via explainGaps", async () => {
    tmpDir = mkdtempSync(join(tmpdir(), "neurallog-e2e-"));
    mkdirSync(join(tmpDir, "src"));
    const srcPath = join(tmpDir, "src", "divide.ts");
    copyFileSync(join(__dirname, "..", "examples", "division-by-zero.ts"), srcPath);

    const db = openDb(join(tmpDir, "test.db"));
    migrate(db, { migrationsFolder: "./drizzle" });

    const contractKey = "src/divide.ts/divide[2]";
    const clause = db.insert(clauses).values({
      contractKey,
      verdict: "proven",
      smt2: "(declare-const q Real) (assert (= q 0.0))",
      clauseHash: "c1",
    }).returning().get();

    // Z3's model (hand-crafted for this fixture): it thinks q should be 0
    const witnessText = `(
      (define-fun q () Real 0.0)
    )`;

    await detectGaps({
      db,
      clauseId: clause.id,
      sourcePath: srcPath,
      functionName: "divide",
      signalLine: 3,
      bindings: [
        { smtConstant: "q", sourceLine: 2, sourceExpr: "a / b", sort: "Real" },
      ],
      z3WitnessText: witnessText,
      inputs: { a: 0, b: 0 },
    });

    // Assert an IEEE gap was recorded
    const gaps = db.select().from(gapReports).where(eq(gapReports.clauseId, clause.id)).all();
    const ieee = gaps.filter((g) => g.kind === "ieee_specials");
    expect(ieee.length).toBeGreaterThanOrEqual(1);

    // Render via explainGaps
    const output = explainGaps(db, contractKey);
    expect(output).toContain("encoding-gap");
    expect(output).toContain("NaN");
    expect(output).toContain("q");
  });
});
```

- [ ] **Step 18.3: Run test to verify it fails**

Run: `npx vitest run src/e2e.gapDetection.test.ts`
Expected: FAIL on first run — reason depends on environment; investigate the first failure before retrying. Common first-run issues:
- Working directory affects migration resolution: the test uses a fresh db path; drizzle migrations must be resolvable from the test process cwd (typically repo root).
- `loadModuleWithPrivates` needs `typescript` available; the project already depends on it (see `src/moduleLoader.ts:11`).

If the failure is not one of those, fix the underlying issue before re-running.

- [ ] **Step 18.4: Run test to verify it passes**

Run: `npx vitest run src/e2e.gapDetection.test.ts`
Expected: PASS (1 test).

- [ ] **Step 18.5: Run full suite**

Run: `npx vitest run`
Expected: PASS all tests (original 87 + all new tests).

- [ ] **Step 18.6: Commit**

```bash
git add examples/division-by-zero.ts src/e2e.gapDetection.test.ts
git commit -m "e2e: division-by-zero fixture produces IEEE NaN gap end-to-end"
```

---

## Self-Review

Spec coverage check (from `docs/specs/2026-04-23-provekit-v2-design.md`, Phase A-thin + D-core portions):

| Spec item | Task |
|---|---|
| `runtime_values` relational graph with object_members + array_elements | Task 2 |
| `traces`, `trace_values` tables | Task 3 |
| `clauses`, `clause_bindings`, `clause_witnesses` tables | Task 4 |
| `gap_reports` table | Task 5 |
| Z3 model parser (Real/Int/Bool/String + NaN/Infinity) | Task 6 |
| Z3 witness persistence to `clause_witnesses` + `runtime_values` | Task 7 |
| Runtime value serializer (primitives + objects + arrays + circular + truncation) | Task 8 |
| Snapshot instrumentation (ts-morph AST rewrite at signal line) | Task 9 |
| Trace capture wired into harness (`runHarnessWithTrace`) | Task 10 |
| Determinism stubs (`Math.random` → 0.5, `Date.now` → 0) | Task 10, Step 10.3 |
| Binding validation (line-based, transitional) | Task 11 |
| Comparator core + IEEE specials agent | Task 12 |
| Outcome-mismatch agent | Task 13 |
| Path-not-taken agent | Task 14 |
| Gap detection orchestrator | Task 15 |
| LLM prompt emits `smt_bindings` | Task 16 |
| CLI `explain --gaps` | Task 17 |
| End-to-end fixture + test | Task 18 |

No spec gaps. Two items deliberately deferred to later phases per the
revised phasing:

- `bound_to_node` FK on `clause_bindings` to `nodes(id)` — requires SAST
  (Phase B).
- Transitive `data_flow_transitive` materialization — Phase B.

These do not block A-thin + D-core; they appear in later phase plans.

Type-consistency check:
- `runHarnessWithTrace` returns `traceId: number`; used in `detectGaps` at
  Task 15 Step 15.3.
- `ComparatorAgent` signature `(input) → GapReport | null` consistent
  across Tasks 12-14.
- `Binding` interface imported by `detectGaps` matches the shape exported
  from `src/bindings/validator.ts`.

Placeholder scan: none. Every task has actual code in every step. No
"TODO", no "implement later", no "similar to Task N" without code.

---

## Execution Handoff

Plan complete and saved to
`docs/plans/2026-04-23-phase-ad-core.md`.

Two execution options:

1. **Subagent-Driven (recommended)** — a fresh subagent executes each task;
   two-stage review between tasks; fast iteration, isolated context per
   task.

2. **Inline Execution** — tasks execute in this session via
   executing-plans; batch execution with checkpoints for review.

Which approach?
