/**
 * Tests for the grammar extractor.
 *
 * Three shapes covered:
 *   1. Drizzle schema extraction against a fixture mirroring promptlib's
 *      sqlite schema (the smoke-test failure case). Verifies both
 *      jsName and sqlName are exposed for the column where they differ
 *      (varsJson → "vars"), so the rendered prompt section pins both.
 *   2. Imports extraction against a project with vitest test files.
 *   3. Fallback behavior on a non-Drizzle, non-vitest fixture: empty
 *      bundle, empty rendered section, no crash.
 */

import { describe, it, expect } from "vitest";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";
import {
  extractDrizzleSchema,
  extractAvailableImports,
  extractGrammarBundle,
  renderGrammarSection,
  parseDrizzleSchema,
} from "./grammarExtractor.js";

function makeFixture(): string {
  return mkdtempSync(join(tmpdir(), "grammar-extractor-"));
}

const PROMPTLIB_SCHEMA = `import { sqliteTable, text, integer, real, index } from "drizzle-orm/sqlite-core";

export const revisions = sqliteTable(
  "revisions",
  {
    id: text("id").primaryKey(),
    artifactKey: text("artifact_key").notNull(),
    version: integer("version").notNull(),
    createdAt: text("created_at").notNull(),
    body: text("body").notNull(),
    source: text("source").notNull(),
  },
  (t) => ({
    byArtifact: index("revisions_by_artifact").on(t.artifactKey, t.version),
  }),
);

export const invocations = sqliteTable(
  "invocations",
  {
    id: text("id").primaryKey(),
    artifactKey: text("artifact_key").notNull(),
    revisionId: text("revision_id").notNull(),
    varsJson: text("vars").notNull(),
    metadataJson: text("metadata").notNull(),
    output: text("output").notNull(),
    date: text("date").notNull(),
  },
  (t) => ({
    byRevision: index("invocations_by_revision").on(t.revisionId),
    byArtifact: index("invocations_by_artifact").on(t.artifactKey, t.date),
  }),
);

export const signals = sqliteTable(
  "signals",
  {
    id: text("id").primaryKey(),
    invocationId: text("invocation_id").notNull(),
    verdict: text("verdict").notNull(),
    reason: text("reason"),
    severity: real("severity"),
    source: text("source"),
    date: text("date").notNull(),
  },
  (t) => ({
    byInvocation: index("signals_by_invocation").on(t.invocationId),
  }),
);
`;

describe("parseDrizzleSchema (promptlib-shaped fixture)", () => {
  it("parses three tables out of the promptlib schema fixture", () => {
    const root = makeFixture();
    try {
      const dir = join(root, "src", "store", "sqlite");
      mkdirSync(dir, { recursive: true });
      const schemaPath = join(dir, "schema.ts");
      writeFileSync(schemaPath, PROMPTLIB_SCHEMA, "utf8");

      const tables = parseDrizzleSchema({ filePath: schemaPath, projectRoot: root });
      expect(tables.map((t) => t.sqlName).sort()).toEqual([
        "invocations",
        "revisions",
        "signals",
      ]);

      const invocations = tables.find((t) => t.sqlName === "invocations")!;
      expect(invocations.jsExportName).toBe("invocations");

      // The smoke-test bug: varsJson (JS) vs "vars" (SQL). Both must surface.
      const varsCol = invocations.columns.find((c) => c.jsName === "varsJson");
      expect(varsCol).toBeDefined();
      expect(varsCol!.sqlName).toBe("vars");
      expect(varsCol!.type).toBe("text");
      expect(varsCol!.constraints).toContain("notNull");

      // metadataJson similarly maps to SQL "metadata".
      const metaCol = invocations.columns.find((c) => c.jsName === "metadataJson");
      expect(metaCol!.sqlName).toBe("metadata");

      // id column is primaryKey.
      const idCol = invocations.columns.find((c) => c.jsName === "id");
      expect(idCol!.constraints).toContain("primaryKey");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("extractDrizzleSchema (full pipeline)", () => {
  it("finds schema via locus-import resolution", () => {
    const root = makeFixture();
    try {
      const dir = join(root, "src", "store", "sqlite");
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, "schema.ts"), PROMPTLIB_SCHEMA, "utf8");
      const locusPath = join(dir, "repositories.ts");
      writeFileSync(
        locusPath,
        `import * as schema from "./schema.js";\nexport const X = 1;`,
        "utf8",
      );
      const tables = extractDrizzleSchema({
        projectRoot: root,
        locusFile: locusPath,
      });
      expect(tables.length).toBe(3);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns empty array on non-Drizzle project", () => {
    const root = makeFixture();
    try {
      mkdirSync(join(root, "src"), { recursive: true });
      writeFileSync(
        join(root, "src", "math.ts"),
        `export function add(a: number, b: number) { return a + b; }`,
        "utf8",
      );
      const tables = extractDrizzleSchema({ projectRoot: root });
      expect(tables).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("extractAvailableImports", () => {
  it("collects imports from existing test files", () => {
    const root = makeFixture();
    try {
      mkdirSync(join(root, "src"), { recursive: true });
      writeFileSync(
        join(root, "src", "math.test.ts"),
        `import { it, expect, describe } from "vitest";
import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import * as schema from "./schema.js";

it("adds", () => { expect(1+1).toBe(2); });`,
        "utf8",
      );
      const imports = extractAvailableImports({ projectRoot: root });

      const vitest = imports.find((i) => i.module === "vitest")!;
      expect(vitest.named.sort()).toEqual(["describe", "expect", "it"]);

      const bsqlite = imports.find((i) => i.module === "better-sqlite3")!;
      expect(bsqlite.hasDefault).toBe(true);

      const drz = imports.find((i) => i.module === "drizzle-orm/better-sqlite3")!;
      expect(drz.named).toContain("drizzle");

      const sch = imports.find((i) => i.module === "./schema.js")!;
      expect(sch.hasNamespace).toBe(true);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns empty on a project with no test files", () => {
    const root = makeFixture();
    try {
      mkdirSync(join(root, "src"), { recursive: true });
      writeFileSync(
        join(root, "src", "math.ts"),
        `export const X = 1;`,
        "utf8",
      );
      const imports = extractAvailableImports({ projectRoot: root });
      expect(imports).toEqual([]);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});

describe("extractGrammarBundle + renderGrammarSection (smoke-test scenario)", () => {
  it("produces a prompt section that pins varsJson AND vars", () => {
    const root = makeFixture();
    try {
      const dir = join(root, "src", "store", "sqlite");
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, "schema.ts"), PROMPTLIB_SCHEMA, "utf8");
      const locusPath = join(dir, "repositories.ts");
      writeFileSync(
        locusPath,
        `import * as schema from "./schema.js";\nexport const X = 1;`,
        "utf8",
      );
      // Add a test file so imports section also populates.
      writeFileSync(
        join(root, "src", "smoke.test.ts"),
        `import { it, expect } from "vitest";\nit("ok", () => { expect(1).toBe(1); });`,
        "utf8",
      );

      const bundle = extractGrammarBundle({
        projectRoot: root,
        locusFile: locusPath,
      });
      const rendered = renderGrammarSection(bundle);

      expect(rendered).toContain("AVAILABLE DATABASE SCHEMA");
      expect(rendered).toContain("table invocations");
      // The load-bearing assertion: varsJson AND "vars" both visible.
      expect(rendered).toContain("varsJson");
      expect(rendered).toContain('"vars"');
      // Same for metadata.
      expect(rendered).toContain("metadataJson");
      expect(rendered).toContain('"metadata"');
      // Imports section pulls in vitest.
      expect(rendered).toContain("AVAILABLE IMPORTS");
      expect(rendered).toContain("vitest");
      // Strong directive language.
      expect(rendered).toMatch(/MUST|Do NOT|fictional/i);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("renders empty string when bundle is empty (non-Drizzle, no tests)", () => {
    const root = makeFixture();
    try {
      mkdirSync(join(root, "src"), { recursive: true });
      writeFileSync(
        join(root, "src", "math.ts"),
        `export function add(a: number, b: number) { return a + b; }`,
        "utf8",
      );
      const bundle = extractGrammarBundle({ projectRoot: root });
      expect(bundle.schema).toEqual([]);
      expect(bundle.imports).toEqual([]);
      expect(renderGrammarSection(bundle)).toBe("");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
