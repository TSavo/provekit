/**
 * ProvekIt Language Server — VS Code (and any LSP-aware editor)
 * surfaces "you are proven wrong" diagnostics and "what must be true
 * here" hover info, in real time as the user edits.
 *
 * Architecture:
 *   - On document open / save: call verifyProject() via the unified
 *     verifier API.
 *   - Filter the resulting ValidityReport to the active document.
 *   - Emit diagnostics for every row whose status is not "holds":
 *       violated   → Error severity (red squiggle)
 *       unresolved → Error severity (extension/bridge not in scope)
 *       decayed    → Warning severity (LLM re-eval needed)
 *       undecidable → Warning severity (solver gave up)
 *   - On hover: surface every invariant whose locus contains the
 *     hover line, formatted as a markdown panel showing intent,
 *     status, and (when violated) the Z3 witness.
 *
 * Implementation note: this LSP is intentionally THIN. It calls into
 * verifyProject() — same function the CLI invokes. No protocol logic
 * lives here. When a new spec lands (e.g. signature verification
 * fully implemented), the verifier picks it up and the LSP inherits.
 *
 * Per the protocol spec stack, an alternative implementation could
 * read the spec docs and implement everything from scratch with NO
 * provekit dependency. That investigation is a separate piece (see
 * spec-from-protocol agent's deliverable when it lands). This LSP
 * is the reference implementation for the TS ecosystem.
 */

import {
  createConnection,
  TextDocuments,
  Diagnostic,
  DiagnosticSeverity,
  Hover,
  MarkupKind,
  ProposedFeatures,
  type InitializeParams,
  type InitializeResult,
  TextDocumentSyncKind,
} from "vscode-languageserver/node.js";
import { TextDocument } from "vscode-languageserver-textdocument";
import { fileURLToPath } from "url";
import { join, dirname } from "path";
import { existsSync } from "fs";
import {
  verifyProject,
  rowsForFile,
  rowsAtLine,
  type InvariantRow,
  type ValidityReport,
} from "../verifier/index.js";

// ---------------------------------------------------------------------------
// LSP wiring
// ---------------------------------------------------------------------------

const connection = createConnection(ProposedFeatures.all);
const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument);

connection.onInitialize((_params: InitializeParams): InitializeResult => {
  return {
    capabilities: {
      textDocumentSync: TextDocumentSyncKind.Incremental,
      hoverProvider: true,
      diagnosticProvider: {
        interFileDependencies: true,
        workspaceDiagnostics: false,
      },
    },
  };
});

// Per-project cache of the most recent ValidityReport. The LSP holds
// this so hover requests can be served instantly without re-running
// verifyProject() per request. Re-verify happens on save.
const reportCache = new Map<string, ValidityReport>();

// ---------------------------------------------------------------------------
// Re-verify on save
// ---------------------------------------------------------------------------

documents.onDidSave(async (event) => {
  const docPath = fileURLToPath(event.document.uri);
  const projectRoot = findProjectRoot(docPath);
  if (!projectRoot) return;

  try {
    const report = await verifyProject(projectRoot, { timeoutMs: 5000 });
    reportCache.set(projectRoot, report);
    publishDiagnosticsForProject(projectRoot, report);
  } catch (err) {
    connection.console.error(
      `provekit verify failed for ${projectRoot}: ${(err as Error).message}`,
    );
  }
});

// Also verify on open so the user gets diagnostics immediately.
documents.onDidOpen(async (event) => {
  const docPath = fileURLToPath(event.document.uri);
  const projectRoot = findProjectRoot(docPath);
  if (!projectRoot) return;
  if (!reportCache.has(projectRoot)) {
    try {
      const report = await verifyProject(projectRoot, { timeoutMs: 5000 });
      reportCache.set(projectRoot, report);
      publishDiagnosticsForProject(projectRoot, report);
    } catch (err) {
      connection.console.error(
        `provekit initial verify failed: ${(err as Error).message}`,
      );
    }
  } else {
    publishDiagnosticsForProject(projectRoot, reportCache.get(projectRoot)!);
  }
});

// ---------------------------------------------------------------------------
// Hover: "what must be true here?"
// ---------------------------------------------------------------------------

connection.onHover((params): Hover | null => {
  const doc = documents.get(params.textDocument.uri);
  if (!doc) return null;
  const docPath = fileURLToPath(params.textDocument.uri);
  const projectRoot = findProjectRoot(docPath);
  if (!projectRoot) return null;
  const report = reportCache.get(projectRoot);
  if (!report) return null;

  // LSP positions are 0-based; the verifier's locus is 1-based.
  const line = params.position.line + 1;
  const rows = rowsAtLine(report, docPath, line);
  if (rows.length === 0) return null;

  const md = renderHover(rows);
  return {
    contents: { kind: MarkupKind.Markdown, value: md },
  };
});

// ---------------------------------------------------------------------------
// Diagnostics emission
// ---------------------------------------------------------------------------

function publishDiagnosticsForProject(projectRoot: string, report: ValidityReport): void {
  // Group rows by file path.
  const byFile = new Map<string, InvariantRow[]>();
  for (const row of report.rows) {
    if (row.status === "holds") continue;
    const list = byFile.get(row.locus.filePath) ?? [];
    list.push(row);
    byFile.set(row.locus.filePath, list);
  }

  // Emit diagnostics for every document the LSP knows about that has
  // failures. Documents with no failures get a clean (empty) diagnostics
  // list so previously-published red squiggles clear.
  for (const doc of documents.all()) {
    const docPath = fileURLToPath(doc.uri);
    const rows = byFile.get(docPath) ?? [];
    const diagnostics: Diagnostic[] = rows.map((row) => ({
      severity: severityFor(row.status),
      range: {
        start: { line: row.locus.startLine - 1, character: 0 },
        end: { line: row.locus.endLine - 1, character: Number.MAX_SAFE_INTEGER },
      },
      message: messageFor(row),
      source: "provekit",
      code: row.invariantId,
    }));
    connection.sendDiagnostics({ uri: doc.uri, diagnostics });
  }
}

function severityFor(status: InvariantRow["status"]): DiagnosticSeverity {
  switch (status) {
    case "violated":
    case "unresolved":
      return DiagnosticSeverity.Error;
    case "decayed":
    case "undecidable":
      return DiagnosticSeverity.Warning;
    case "holds":
      return DiagnosticSeverity.Hint;
  }
}

function messageFor(row: InvariantRow): string {
  const head = `Invariant "${row.intent}" — ${row.status.toUpperCase()}`;
  const reason = row.reason ? `\n  ${row.reason}` : "";
  const witness = row.witness !== undefined
    ? `\n  Z3 witness: ${truncate(JSON.stringify(row.witness), 200)}`
    : "";
  return head + reason + witness;
}

function truncate(s: string, n: number): string {
  if (s.length <= n) return s;
  return s.slice(0, n - 1) + "…";
}

// ---------------------------------------------------------------------------
// Hover rendering
// ---------------------------------------------------------------------------

function renderHover(rows: InvariantRow[]): string {
  const parts: string[] = [];
  parts.push("**ProvekIt — what must be true here**");
  parts.push("");
  for (const row of rows) {
    parts.push(`### ${statusEmoji(row.status)} ${row.intent}`);
    parts.push("");
    parts.push(`- **Status:** \`${row.status}\``);
    parts.push(`- **Invariant ID:** \`${row.invariantId}\``);
    parts.push(`- **Locus:** \`${row.locus.filePath}:${row.locus.startLine}\` (\`${row.locus.function ?? "(no function)"}\`)`);
    if (row.reason) parts.push(`- **Reason:** ${row.reason}`);
    if (row.witness !== undefined) {
      parts.push(`- **Z3 witness:** \`${truncate(JSON.stringify(row.witness), 200)}\``);
    }
    parts.push("");
  }
  return parts.join("\n");
}

function statusEmoji(status: InvariantRow["status"]): string {
  switch (status) {
    case "holds": return "✅";
    case "violated": return "❌";
    case "decayed": return "🔄";
    case "unresolved": return "⛓️";
    case "undecidable": return "❓";
  }
}

// ---------------------------------------------------------------------------
// Project root discovery
// ---------------------------------------------------------------------------

function findProjectRoot(startPath: string): string | null {
  let dir = dirname(startPath);
  // Walk up looking for .provekit/ directory.
  while (dir !== "/" && dir !== "" && dir !== ".") {
    if (existsSync(join(dir, ".provekit"))) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

documents.listen(connection);
connection.listen();
