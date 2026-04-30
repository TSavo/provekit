/**
 * ProvekIt Language Server — surfaces bridge enforcement diagnostics.
 *
 * On save: runs runBridgeEnforcement() against the project, renders
 * each unsatisfied / unresolved-target / lift-error row as a workspace
 * diagnostic. Bridge call sites that discharge cleanly produce no
 * diagnostic.
 *
 * v2 of the LSP. The legacy per-invariant ValidityReport surface
 * was removed alongside the legacy verifier. Bridge enforcement is
 * the verifier; diagnostics map per-callsite, not per-invariant.
 */

import {
  createConnection,
  TextDocuments,
  Diagnostic,
  DiagnosticSeverity,
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
  runBridgeEnforcement,
  type BridgeEnforcementReport,
} from "../verifier/index.js";

const connection = createConnection(ProposedFeatures.all);
const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument);

connection.onInitialize((_params: InitializeParams): InitializeResult => {
  return {
    capabilities: {
      textDocumentSync: TextDocumentSyncKind.Incremental,
      diagnosticProvider: {
        interFileDependencies: true,
        workspaceDiagnostics: false,
      },
    },
  };
});

const reportCache = new Map<string, BridgeEnforcementReport>();

documents.onDidSave(async (event) => {
  const docPath = fileURLToPath(event.document.uri);
  const projectRoot = findProjectRoot(docPath);
  if (!projectRoot) return;
  try {
    const report = await runBridgeEnforcement(projectRoot);
    reportCache.set(projectRoot, report);
    publishDiagnosticsForProject(projectRoot, report);
  } catch (err) {
    connection.console.error(
      `provekit verify failed for ${projectRoot}: ${(err as Error).message}`,
    );
  }
});

documents.onDidOpen(async (event) => {
  const docPath = fileURLToPath(event.document.uri);
  const projectRoot = findProjectRoot(docPath);
  if (!projectRoot) return;
  if (!reportCache.has(projectRoot)) {
    try {
      const report = await runBridgeEnforcement(projectRoot);
      reportCache.set(projectRoot, report);
      publishDiagnosticsForProject(projectRoot, report);
    } catch (err) {
      connection.console.error(
        `provekit verify (initial) failed: ${(err as Error).message}`,
      );
    }
  } else {
    publishDiagnosticsForProject(projectRoot, reportCache.get(projectRoot)!);
  }
});

function publishDiagnosticsForProject(
  _projectRoot: string,
  report: BridgeEnforcementReport,
): void {
  // The bridge enforcement report is per-callsite (per property memento
  // in a .proof file), not per-source-file. v2 of the LSP doesn't yet
  // map call sites back to TS source positions; diagnostics are emitted
  // against any open document with a generic header. A follow-up will
  // walk the property memento's binding scope to recover source loci.
  for (const doc of documents.all()) {
    const diagnostics: Diagnostic[] = [];
    for (const row of report.rows) {
      if (row.status === "discharged") continue;
      const cs = row.callsite as {
        bridgeIrName: string;
        propertyName: string;
        propertyCid: string;
      };
      const reasonSuffix = row.reason ? ` — ${row.reason}` : "";
      diagnostics.push({
        severity:
          row.status === "undecidable"
            ? DiagnosticSeverity.Warning
            : DiagnosticSeverity.Error,
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 0 },
        },
        message: `${cs.bridgeIrName} in ${cs.propertyName}: ${row.status}${reasonSuffix}`,
        source: "provekit",
        code: cs.propertyCid.slice(0, 12),
      });
    }
    connection.sendDiagnostics({ uri: doc.uri, diagnostics });
  }
}

function findProjectRoot(startPath: string): string | null {
  let dir = dirname(startPath);
  while (dir !== "/" && dir !== "" && dir !== ".") {
    if (existsSync(join(dir, ".provekit"))) return dir;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

documents.listen(connection);
connection.listen();
