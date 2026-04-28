/**
 * Grammar extractor for C-stage prompts.
 *
 * The architectural principle (per CDD): the LLM shouldn't have to remember
 * the available constraints; the prompt should ENUMERATE them. State the
 * available grammar (schema, imports, framework verbs) explicitly so the
 * model's choice space at generation time matches reality.
 *
 * Concretely solves: C5 generates regression tests against codebases the
 * LLM has never seen the schema for. Without an enumerated schema, the
 * model fabricates plausible-looking column names (e.g. it wrote `vars`
 * as a column on `invocations`, but the Drizzle schema actually has
 * `varsJson: text("vars")` — JS property is `varsJson`, SQL column is
 * `"vars"`). Either name in isolation is wrong depending on the access
 * surface (Drizzle property vs raw SQL); enumerating both kills both
 * failure modes.
 *
 * v1 scope:
 *   - extractDrizzleSchema(): parses sqliteTable("name", { ... }) blocks
 *     in nearby schema files. Returns Table[] with both JS field names
 *     and SQL column names exposed.
 *   - extractAvailableImports(): scans existing *.test.ts files in the
 *     project and gathers their import statements as the canonical "what
 *     this project's tests are allowed to import" set.
 *   - extractGrammarBundle(): one-stop call for C5 that returns both.
 *
 * Fallback contract: every extractor returns an empty result on failure.
 * No throws. The prompt builder skips the section when the result is
 * empty so non-Drizzle / non-vitest projects produce a clean prompt.
 *
 * Future C-stages (C1 SMT operator surface, C3 locus APIs, C6 DSL
 * grammar) consume the same shape with different filters.
 */

import { readFileSync, readdirSync, statSync, existsSync } from "fs";
import { join, dirname, relative, basename } from "path";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/**
 * One column inside a Drizzle table declaration.
 *
 * We expose both names because the bug-shape that motivated this module
 * (LLM writing `vars` instead of `varsJson` or vice versa) is exactly
 * the JS-vs-SQL ambiguity. Either name in isolation lets the model pick
 * wrong; both together pin it.
 */
export interface SchemaColumn {
  /** JS property name on the Drizzle exported table (what `schema.invocations.varsJson` accesses). */
  jsName: string;
  /** Underlying SQL column name (what `SELECT vars FROM invocations` references). */
  sqlName: string;
  /** Drizzle column type, e.g. "text", "integer", "real". */
  type: string;
  /** Constraints captured from the chained .modifiers() (e.g. "primary key", "not null"). */
  constraints: string[];
}

/**
 * One Drizzle table declaration.
 */
export interface SchemaTable {
  /** SQL table name from sqliteTable("invocations", ...). */
  sqlName: string;
  /** JS export name from `export const invocations = sqliteTable(...)`. */
  jsExportName: string;
  /** Path of the source file that declared the table, relative to projectRoot. */
  sourceFile: string;
  columns: SchemaColumn[];
}

/**
 * One import statement gathered from existing test files. Used to teach
 * the LLM what's in the project's idiom — which test framework, ORM,
 * helpers, utilities are actually present and reachable.
 */
export interface AvailableImport {
  /** Module specifier as written, e.g. "vitest", "better-sqlite3", "drizzle-orm/better-sqlite3". */
  module: string;
  /** Named imports gathered across all test files for this module. */
  named: string[];
  /** True if any test file imported this module's default export. */
  hasDefault: boolean;
  /** True if any test file did `import * as X from "...";`. */
  hasNamespace: boolean;
}

/**
 * Bundle of grammar artifacts for a C-stage prompt.
 */
export interface GrammarBundle {
  /** All Drizzle tables found in the project (empty if none / not Drizzle). */
  schema: SchemaTable[];
  /** Imports observed across the project's existing test files. */
  imports: AvailableImport[];
}

// ---------------------------------------------------------------------------
// Schema extraction
// ---------------------------------------------------------------------------

/**
 * Locate the schema file(s) in a project.
 *
 * Strategy: prefer the schema imported by the locus file (most precise);
 * fall back to a small set of well-known paths. Returns absolute paths
 * to the schema files we should parse.
 */
export function findSchemaFiles(args: {
  projectRoot: string;
  locusFile?: string;
}): string[] {
  const { projectRoot, locusFile } = args;
  const found = new Set<string>();

  // 1. If a locus file is provided, parse its imports for "./schema" or
  //    "./schema.js" style references — that's the schema the locus is
  //    actually using.
  if (locusFile && existsSync(locusFile)) {
    try {
      const src = readFileSync(locusFile, "utf8");
      const importRe = /import[^;]*?from\s+['"]([^'"]+)['"]/g;
      let m: RegExpExecArray | null;
      while ((m = importRe.exec(src)) !== null) {
        const spec = m[1];
        // Only relative paths can point at a schema file in-tree.
        if (!spec.startsWith(".")) continue;
        if (!/schema(\.js|\.ts)?$/.test(spec)) continue;
        const base = spec.replace(/\.(js|ts)$/, "");
        const cand = [
          join(dirname(locusFile), `${base}.ts`),
          join(dirname(locusFile), `${base}.tsx`),
        ];
        for (const c of cand) {
          if (existsSync(c)) found.add(c);
        }
      }
    } catch {
      // Swallow — fallback paths are next.
    }
  }

  // 2. Well-known schema locations under projectRoot.
  const fallbacks = [
    "src/store/sqlite/schema.ts",
    "src/db/schema.ts",
    "src/schema.ts",
    "src/store/schema.ts",
  ];
  for (const f of fallbacks) {
    const abs = join(projectRoot, f);
    if (existsSync(abs)) found.add(abs);
  }

  // 3. Shallow walk under src/ for any file literally named schema.ts.
  //    We cap depth to avoid pathological traversals on huge repos.
  const srcRoot = join(projectRoot, "src");
  if (existsSync(srcRoot)) {
    walkForSchema(srcRoot, found, 0, 4);
  }

  return Array.from(found);
}

function walkForSchema(
  dir: string,
  out: Set<string>,
  depth: number,
  maxDepth: number,
): void {
  if (depth > maxDepth) return;
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (name === "node_modules" || name === ".git" || name === "dist") continue;
    const abs = join(dir, name);
    let st;
    try {
      st = statSync(abs);
    } catch {
      continue;
    }
    if (st.isDirectory()) {
      walkForSchema(abs, out, depth + 1, maxDepth);
    } else if (name === "schema.ts") {
      out.add(abs);
    }
  }
}

/**
 * Parse Drizzle sqliteTable declarations out of a single schema source file.
 *
 * Tolerant regex parser. Skips anything that doesn't match the canonical
 * `sqliteTable("name", { columns })` shape. The schema-mapping regex is
 * deliberately lax on whitespace/comments inside the column block.
 *
 * Returns a list of tables; empty list on parse failure or no matches.
 */
export function parseDrizzleSchema(args: {
  filePath: string;
  projectRoot: string;
}): SchemaTable[] {
  const { filePath, projectRoot } = args;
  let src: string;
  try {
    src = readFileSync(filePath, "utf8");
  } catch {
    return [];
  }

  // Identify each `export const X = sqliteTable("name", { ... })` block.
  // We can't use a single regex for the brace-balanced { ... } body, so:
  //   1. Match the prefix `export const NAME = sqliteTable("SQL_NAME", {`.
  //   2. Walk the source from the `{` until braces balance.
  const tables: SchemaTable[] = [];
  const headerRe =
    /export\s+const\s+(\w+)\s*=\s*sqliteTable\s*\(\s*['"]([^'"]+)['"]\s*,\s*\{/g;
  let header: RegExpExecArray | null;
  while ((header = headerRe.exec(src)) !== null) {
    const jsExportName = header[1];
    const sqlName = header[2];
    const bodyStart = headerRe.lastIndex; // first char after `{`
    const bodyEnd = findMatchingBrace(src, bodyStart - 1);
    if (bodyEnd === -1) continue;
    const body = src.slice(bodyStart, bodyEnd);
    const columns = parseColumnBlock(body);
    if (columns.length === 0) continue;
    tables.push({
      sqlName,
      jsExportName,
      sourceFile: relative(projectRoot, filePath) || basename(filePath),
      columns,
    });
  }

  return tables;
}

/**
 * Given a string and the index of an opening `{`, return the index of
 * the matching `}`. Returns -1 if unbalanced.
 *
 * Naive: doesn't strip strings/comments. Drizzle column blocks rarely
 * contain `{`/`}` inside string literals, and false positives produce
 * truncated parses (which we treat as "no columns" and skip), not
 * crashes.
 */
function findMatchingBrace(src: string, openIdx: number): number {
  let depth = 0;
  for (let i = openIdx; i < src.length; i++) {
    const c = src[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Parse the comma-separated `field: type("sql_name", ...).modifier()` lines
 * inside a sqliteTable column block.
 *
 * Each line: `<jsName>: <type>("<sqlName>"[, ...])[.constraint()...]`.
 * If the SQL name is omitted (some Drizzle versions allow that), fall
 * back to jsName.
 */
function parseColumnBlock(body: string): SchemaColumn[] {
  const cols: SchemaColumn[] = [];

  // Strip line comments to avoid matching commented-out columns.
  const cleaned = body.replace(/\/\/.*$/gm, "");

  // Match each `name: type("sql"...)` then capture trailing modifiers up
  // to the next comma at depth 0 or end-of-block.
  //
  // We use a top-down scan: split on commas not inside parens.
  const segments = splitTopLevelCommas(cleaned);
  for (const seg of segments) {
    const trimmed = seg.trim();
    if (!trimmed) continue;
    // Skip trailing-block items that aren't column declarations
    // (e.g. an inline secondary callback). They start with `(` or
    // don't have a `:` at top level.
    const colonIdx = trimmed.indexOf(":");
    if (colonIdx === -1) continue;
    const jsName = trimmed.slice(0, colonIdx).trim();
    if (!/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(jsName)) continue;
    const rhs = trimmed.slice(colonIdx + 1).trim();

    // Match `<type>(...)` at the start of the RHS.
    const typeMatch = rhs.match(/^(\w+)\s*\(/);
    if (!typeMatch) continue;
    const type = typeMatch[1];

    // SQL column name is the first string literal inside the call.
    const sqlMatch = rhs.match(/^\w+\s*\(\s*['"]([^'"]+)['"]/);
    const sqlName = sqlMatch ? sqlMatch[1] : jsName;

    // Constraints: any chained method calls after the closing paren of
    // the type call, e.g. .primaryKey(), .notNull(), .default(...).
    const constraints: string[] = [];
    const constraintRe = /\.([A-Za-z_$][A-Za-z0-9_$]*)\s*\(/g;
    let cm: RegExpExecArray | null;
    while ((cm = constraintRe.exec(rhs)) !== null) {
      constraints.push(cm[1]);
    }

    cols.push({ jsName, sqlName, type, constraints });
  }

  return cols;
}

/**
 * Split a comma-separated list at depth-0 commas only. Tracks paren and
 * bracket depth; ignores commas inside strings.
 */
function splitTopLevelCommas(src: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let strCh: string | null = null;
  let start = 0;
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (strCh) {
      if (c === "\\") {
        i++;
        continue;
      }
      if (c === strCh) strCh = null;
      continue;
    }
    if (c === "'" || c === '"' || c === "`") {
      strCh = c;
      continue;
    }
    if (c === "(" || c === "[" || c === "{") depth++;
    else if (c === ")" || c === "]" || c === "}") depth--;
    else if (c === "," && depth === 0) {
      out.push(src.slice(start, i));
      start = i + 1;
    }
  }
  if (start < src.length) out.push(src.slice(start));
  return out;
}

/**
 * Top-level: extract every Drizzle schema table reachable from the
 * project. Combines findSchemaFiles + parseDrizzleSchema + dedup.
 */
export function extractDrizzleSchema(args: {
  projectRoot: string;
  locusFile?: string;
}): SchemaTable[] {
  const files = findSchemaFiles(args);
  const seen = new Set<string>();
  const tables: SchemaTable[] = [];
  for (const f of files) {
    for (const t of parseDrizzleSchema({ filePath: f, projectRoot: args.projectRoot })) {
      const key = `${t.sourceFile}::${t.jsExportName}`;
      if (seen.has(key)) continue;
      seen.add(key);
      tables.push(t);
    }
  }
  return tables;
}

// ---------------------------------------------------------------------------
// Imports extraction
// ---------------------------------------------------------------------------

/**
 * Walk the project for *.test.ts files and extract their imports as a
 * canonical "what's idiomatic in this project's tests" set. Capped depth
 * + skip list keeps the walk fast on large repos.
 */
export function extractAvailableImports(args: {
  projectRoot: string;
  /** Optional cap on test files to scan (default 30). */
  maxFiles?: number;
}): AvailableImport[] {
  const { projectRoot, maxFiles = 30 } = args;
  const testFiles: string[] = [];
  const srcRoot = join(projectRoot, "src");
  if (existsSync(srcRoot)) {
    walkForTestFiles(srcRoot, testFiles, 0, 5, maxFiles);
  }
  const testsRoot = join(projectRoot, "tests");
  if (existsSync(testsRoot) && testFiles.length < maxFiles) {
    walkForTestFiles(testsRoot, testFiles, 0, 5, maxFiles);
  }

  const byModule = new Map<string, AvailableImport>();
  for (const f of testFiles) {
    let src: string;
    try {
      src = readFileSync(f, "utf8");
    } catch {
      continue;
    }
    parseImportsInto(src, byModule);
  }
  return Array.from(byModule.values()).sort((a, b) =>
    a.module.localeCompare(b.module),
  );
}

function walkForTestFiles(
  dir: string,
  out: string[],
  depth: number,
  maxDepth: number,
  maxFiles: number,
): void {
  if (depth > maxDepth) return;
  if (out.length >= maxFiles) return;
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }
  for (const name of entries) {
    if (out.length >= maxFiles) return;
    if (name === "node_modules" || name === ".git" || name === "dist") continue;
    const abs = join(dir, name);
    let st;
    try {
      st = statSync(abs);
    } catch {
      continue;
    }
    if (st.isDirectory()) {
      walkForTestFiles(abs, out, depth + 1, maxDepth, maxFiles);
    } else if (/\.(test|spec)\.tsx?$/.test(name)) {
      // Skip provekit's own regression test files — they're substrate,
      // not the project's idiom.
      if (name.endsWith(".regression.test.ts")) continue;
      out.push(abs);
    }
  }
}

/**
 * Parse `import ... from "module"` statements out of a source string and
 * merge them into the running map keyed by module specifier.
 *
 * Handles:
 *   import X from "m";              → hasDefault
 *   import { a, b } from "m";       → named: [a, b]
 *   import { a as a2 } from "m";    → named: [a]  (we keep the original,
 *                                     since the LLM should know what's
 *                                     exported, not what aliases this
 *                                     test happened to use)
 *   import * as X from "m";         → hasNamespace
 *   import X, { a } from "m";       → both
 */
function parseImportsInto(
  src: string,
  out: Map<string, AvailableImport>,
): void {
  const re = /import\s+([^;'"]+?)\s+from\s+['"]([^'"]+)['"]/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const clause = m[1].trim();
    const module = m[2];
    const entry = out.get(module) ?? {
      module,
      named: [],
      hasDefault: false,
      hasNamespace: false,
    };

    // Strip `type ` modifier.
    const cleaned = clause.replace(/^type\s+/, "");

    // Namespace: `* as X`
    if (/^\*\s+as\s+\w+$/.test(cleaned)) {
      entry.hasNamespace = true;
    } else {
      // Possibly `Default, { a, b }` or `{ a, b }` or `Default`.
      const braceIdx = cleaned.indexOf("{");
      if (braceIdx === -1) {
        // Default-only: `Default` (single identifier).
        if (/^\w+$/.test(cleaned)) {
          entry.hasDefault = true;
        }
      } else {
        const beforeBrace = cleaned.slice(0, braceIdx).replace(",", "").trim();
        if (beforeBrace && /^\w+$/.test(beforeBrace)) {
          entry.hasDefault = true;
        }
        const closing = cleaned.indexOf("}", braceIdx);
        if (closing !== -1) {
          const namedList = cleaned.slice(braceIdx + 1, closing);
          const names = namedList
            .split(",")
            .map((s) => s.trim().replace(/^type\s+/, ""))
            .map((s) => s.split(/\s+as\s+/)[0].trim())
            .filter((s) => /^\w+$/.test(s));
          for (const n of names) {
            if (!entry.named.includes(n)) entry.named.push(n);
          }
        }
      }
    }

    out.set(module, entry);
  }
}

// ---------------------------------------------------------------------------
// Top-level bundle
// ---------------------------------------------------------------------------

/**
 * One-shot grammar bundle for a C-stage prompt.
 *
 * Returns { schema: [], imports: [] } when the project has no Drizzle
 * schema and no test files. Callers should check for empty arrays and
 * skip the corresponding prompt section to avoid teaching the LLM with
 * an empty list.
 */
export function extractGrammarBundle(args: {
  projectRoot: string;
  locusFile?: string;
}): GrammarBundle {
  return {
    schema: extractDrizzleSchema(args),
    imports: extractAvailableImports({ projectRoot: args.projectRoot }),
  };
}

// ---------------------------------------------------------------------------
// Prompt rendering
// ---------------------------------------------------------------------------

/**
 * Render the GrammarBundle as a prompt-ready string with strong directive
 * language. Returns the empty string when the bundle has nothing to
 * teach (no schema, no imports) so the caller's prompt template can
 * unconditionally interpolate it without an empty section appearing.
 */
export function renderGrammarSection(bundle: GrammarBundle): string {
  const parts: string[] = [];

  if (bundle.schema.length > 0) {
    parts.push(renderSchemaSection(bundle.schema));
  }
  if (bundle.imports.length > 0) {
    parts.push(renderImportsSection(bundle.imports));
  }

  return parts.join("\n\n");
}

function renderSchemaSection(tables: SchemaTable[]): string {
  const lines: string[] = [];
  lines.push("== AVAILABLE DATABASE SCHEMA ==");
  lines.push("");
  lines.push(
    "The following tables and columns are the ONLY ones available in this project's database.",
  );
  lines.push(
    "Your test MUST reference only these names. Inventing column or table names will fail oracle #9a",
  );
  lines.push("with errors like `table X has no column named Y`.");
  lines.push("");
  lines.push(
    "Each column shows BOTH names: the JS property name (used via Drizzle, e.g.",
  );
  lines.push(
    "`schema.invocations.varsJson`) AND the underlying SQL column name (used in raw SQL",
  );
  lines.push(
    '`SELECT vars FROM ...`). Pick the right one for the access pattern you use.',
  );
  lines.push("");

  for (const t of tables) {
    lines.push(`table ${t.sqlName}  (drizzle export: schema.${t.jsExportName}, declared in ${t.sourceFile})`);
    for (const c of t.columns) {
      const constraintsStr =
        c.constraints.length > 0 ? `  [${c.constraints.join(", ")}]` : "";
      const same = c.jsName === c.sqlName;
      const namePart = same
        ? `${c.jsName}`
        : `${c.jsName}  (SQL column: "${c.sqlName}")`;
      lines.push(`  ${namePart}: ${c.type}${constraintsStr}`);
    }
    lines.push("");
  }

  lines.push("DIRECTIVE: The generated test MUST use only the JS property names above when");
  lines.push("accessing columns through Drizzle (e.g. `schema.invocations.varsJson`), and only");
  lines.push("the SQL column names above when writing raw SQL. Do NOT invent column names.");
  lines.push("If a column you would expect doesn't appear in the list, the test must work with");
  lines.push("the columns that DO exist; do not reference fictional columns.");

  return lines.join("\n");
}

function renderImportsSection(imports: AvailableImport[]): string {
  const lines: string[] = [];
  lines.push("== AVAILABLE IMPORTS ==");
  lines.push("");
  lines.push(
    "These modules and symbols were observed across this project's existing test files.",
  );
  lines.push(
    "They are known to be installed and reachable. Prefer them over guessed alternatives.",
  );
  lines.push("");
  for (const i of imports) {
    const bits: string[] = [];
    if (i.hasDefault) bits.push("default");
    if (i.hasNamespace) bits.push("* as ns");
    if (i.named.length > 0) bits.push(`{ ${i.named.join(", ")} }`);
    const detail = bits.length > 0 ? bits.join(", ") : "(no symbols observed)";
    lines.push(`  from "${i.module}":  ${detail}`);
  }
  lines.push("");
  lines.push(
    "DIRECTIVE: Prefer these import paths and symbols. If you need something that isn't",
  );
  lines.push(
    "listed here, only invent the import if the symbol is part of the module under test",
  );
  lines.push(
    "(its public API). Do not invent imports from third-party packages not seen above.",
  );

  return lines.join("\n");
}
