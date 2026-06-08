// Tiny LSP server, spec-only. Run with `node server.mjs --stdio`.
// Surfaces the propertyHash CID as a hover card when the user's cursor is over
// a JSON object with kind:"property". Builds entirely from the protocol spec
// stack: no sugar framework dependency.
//
// This is a PROTOTYPE. It demonstrates the architectural path. To grow it into
// a production LSP, see README.md §"What this prototype is not."

import {
  createConnection,
  ProposedFeatures,
  TextDocuments,
  TextDocumentSyncKind,
  DiagnosticSeverity,
} from "vscode-languageserver/node.js";
import { TextDocument } from "vscode-languageserver-textdocument";

import { parseDocument, GrammarParseError } from "./parse.mjs";
import { propertyHash } from "./canonicalize.mjs";

const connection = createConnection(ProposedFeatures.all);
const documents = new TextDocuments(TextDocument);

connection.onInitialize(() => ({
  capabilities: {
    textDocumentSync: TextDocumentSyncKind.Incremental,
    hoverProvider: true,
  },
}));

documents.onDidChangeContent(({ document }) => publishDiagnostics(document));
documents.onDidOpen(({ document }) => publishDiagnostics(document));

function publishDiagnostics(document) {
  if (!document.uri.endsWith(".json")) return;
  const text = document.getText();
  const diagnostics = [];
  try {
    parseDocument(text);
  } catch (err) {
    if (err instanceof GrammarParseError) {
      diagnostics.push({
        severity: DiagnosticSeverity.Error,
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        message: `IR grammar violation: ${err.message}`,
        source: "lsp-from-spec",
      });
    } else {
      diagnostics.push({
        severity: DiagnosticSeverity.Error,
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
        message: `JSON parse error: ${err.message}`,
        source: "lsp-from-spec",
      });
    }
  }
  connection.sendDiagnostics({ uri: document.uri, diagnostics });
}

connection.onHover(({ textDocument }) => {
  const document = documents.get(textDocument.uri);
  if (!document) return null;
  let decls;
  try {
    decls = parseDocument(document.getText());
  } catch {
    return null;
  }
  const lines = ["**Sugar invariant file**", ""];
  for (const d of decls) {
    if (d.kind !== "property") continue;
    const cid = propertyHash(d.formula);
    lines.push(`- **${d.name}** (property)`);
    lines.push(`  - propertyHash (jcs-rfc8785): \`${cid}\``);
    lines.push(`  - source: canonicalization-grammar §3, §7.3, §9`);
  }
  return { contents: { kind: "markdown", value: lines.join("\n") } };
});

documents.listen(connection);
connection.listen();
