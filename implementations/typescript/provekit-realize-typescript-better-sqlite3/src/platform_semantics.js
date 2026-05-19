// SPDX-License-Identifier: Apache-2.0
//
// Binding-kit platform semantics declaration for the TypeScript better-sqlite3 library.
//
// Implements the provekit.plugin.platform_semantics RPC method (PEP 1.7.0).
// Returns the JSON payload for the "typescript-better-sqlite3" binding target.
//
// RowIdMechanism = LastInsertRowid: the library exposes the inserted row id
// as `stmt.run(...).lastInsertRowid`, which reads connection-global mutable
// state maintained by SQLite and set by the most recent INSERT on that
// connection. This is structurally different from the PostgreSQL mechanism
// (RETURNING clause) and requires a distinct migration pattern.
//
// CID computation follows the substrate spec:
//   DimensionValueMemento CID = blake3-512(JCS(memento WITHOUT cid + kit_cid))
//   PlatformSemanticTag CID   = blake3-512(JCS(tag WITHOUT cid + kit_cid))

"use strict";

const { blake3 } = require("@noble/hashes/blake3.js");

const KIT_ID = "provekit-binding-better-sqlite3@0.1.0";

// CID for concept:insert-and-get-id, minted from its AlgorithmMemento via JCS+blake3-512.
const CONCEPT_INSERT_AND_GET_ID_CID =
  "blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca";

// kit_cid is provenance metadata only (elided from CID computation per substrate spec).
const KIT_CID = "blake3-512:" + _hexOf(blake3(
  new TextEncoder().encode(KIT_ID),
  { dkLen: 64 }
));

// Cached result to avoid repeated computation.
let _cached = null;

/** Returns the platform_semantics declaration for the better-sqlite3 binding kit. */
function declaration() {
  if (_cached !== null) return _cached;

  const dimValues = dimensionValues();
  const rowIdCid = dimValues.find((d) => d.dimension_name === "RowIdMechanism").cid;

  const tags = [
    _tag(CONCEPT_INSERT_AND_GET_ID_CID, { RowIdMechanism: rowIdCid }),
  ];

  _cached = { tags, dimension_values: dimValues };
  return _cached;
}

/** Returns the dimension values for the better-sqlite3 binding kit. */
function dimensionValues() {
  return [
    // LastInsertRowid: row id is sourced from connection-global mutable state
    // (SQLite last_insert_rowid) set by the most recent INSERT.
    _dimValue("RowIdMechanism", "LastInsertRowid", {
      kind: "atomic",
      name: "row_id_source",
      args: [
        {
          kind: "ctor",
          name: "last_insert_rowid",
          args: [
            { kind: "ctor", name: "connection_state_at_call_return", args: [] },
          ],
        },
      ],
    }),
  ];
}

// --- Helpers ---

function _dimValue(dimensionName, valueName, compareTo) {
  const forCid = {
    compare_to: compareTo,
    dimension_name: dimensionName,
    kind: "platform-dimension-value",
    schemaVersion: "1.0.0",
    value_name: valueName,
  };
  return {
    compare_to: compareTo,
    dimension_name: dimensionName,
    kind: "platform-dimension-value",
    kit_cid: KIT_CID,
    schemaVersion: "1.0.0",
    value_name: valueName,
    cid: _cidOf(forCid),
  };
}

function _tag(opCid, dimensions) {
  const forCid = {
    dimensions,
    kind: "platform-semantic-tag",
    op_cid: opCid,
    schemaVersion: "1.0.0",
  };
  return {
    dimensions,
    kind: "platform-semantic-tag",
    kit_cid: KIT_CID,
    op_cid: opCid,
    schemaVersion: "1.0.0",
    cid: _cidOf(forCid),
  };
}

function _cidOf(obj) {
  const canonical = _jcs(obj);
  const bytes = new TextEncoder().encode(canonical);
  const hash = blake3(bytes, { dkLen: 64 });
  return "blake3-512:" + _hexOf(hash);
}

function _jcs(obj) {
  return JSON.stringify(_sortKeys(obj));
}

function _sortKeys(val) {
  if (val === null || typeof val !== "object") return val;
  if (Array.isArray(val)) return val.map(_sortKeys);
  return Object.fromEntries(
    Object.keys(val).sort().map((k) => [k, _sortKeys(val[k])])
  );
}

function _hexOf(bytes) {
  return Array.from(bytes).map((b) => b.toString(16).padStart(2, "0")).join("");
}

module.exports = { declaration, dimensionValues, CONCEPT_INSERT_AND_GET_ID_CID, KIT_CID };
