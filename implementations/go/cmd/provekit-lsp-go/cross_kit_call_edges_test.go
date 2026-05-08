// cross_kit_call_edges_test.go — conformance tests for call-edge stream
// emission per protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1.
//
// Test 1: same-kit call edge — Go function calling another Go function.
//
//	Both sourceContractCid and targetContractCid are populated.
//
// Test 2: cross-kit cgo call edge — Go function calling C.rustFunc(...).
//
//	targetContractCid is null; targetSymbol is "rust-kit:rustFunc".
//
// Test 3: JCS bytes are byte-deterministic across two runs of the same
//
//	source fixture.
package main

import (
	"encoding/json"
	"strings"
	"testing"
)

// sameKitSource is a Go file with two //provekit:contract annotated
// functions where CallerFn calls CalleeFn. Both have contracts; the
// lifter should emit one same-kit call-edge for the call site.
const sameKitSource = `package demo

//provekit:contract
func CalleeFn(x int) int { return x + 1 }

//provekit:contract
func CallerFn(x int) int { return CalleeFn(x) }
`

// cgoSource is a Go file with a //provekit:contract annotated Go
// function that calls C.rustFunc via cgo. The preamble includes
// rust_callee.h so the cgo resolver maps the call to "rust-kit".
// The lifter should emit a cross-kit call-edge with targetContractCid
// null and targetSymbol "rust-kit:rustFunc".
const cgoSource = `package demo

/*
#include "rust_callee.h"
#include <stdint.h>
extern int64_t rustFunc(int64_t n);
*/
import "C"

//provekit:contract
func GoWrapper(n int) int {
	return int(C.rustFunc(C.int64_t(n)))
}
`

// TestCallEdge_SameKit asserts that a Go function calling another Go
// function (both with //provekit:contract) emits a call-edge with
// both sourceContractCid and targetContractCid populated.
func TestCallEdge_SameKit(t *testing.T) {
	done := capture()
	msg := json.RawMessage(mustMarshal(parseParams{Path: "demo.go", Source: sameKitSource}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 10.0, Method: "parse", Params: msg}))
	resp := done()

	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}

	m := resultMap(resp)

	// Expect 2 contract declarations (CalleeFn, CallerFn).
	declList, ok := m["declarations"].([]interface{})
	if !ok {
		t.Fatal("declarations not a list")
	}
	if len(declList) != 2 {
		t.Fatalf("expected 2 declarations, got %d", len(declList))
	}

	// Expect at least 1 call-edge in the callEdges stream.
	edgesRaw, ok := m["callEdges"]
	if !ok {
		t.Fatal("callEdges field missing from parse result")
	}
	edgeList, ok := edgesRaw.([]interface{})
	if !ok {
		t.Fatalf("callEdges not a list: %T", edgesRaw)
	}
	if len(edgeList) == 0 {
		t.Fatal("expected at least 1 call-edge for same-kit call, got 0")
	}

	// Find the call edge from CallerFn -> CalleeFn.
	var found bool
	for _, e := range edgeList {
		edge, ok := e.(map[string]interface{})
		if !ok {
			continue
		}
		if edge["kind"] != "call-edge" {
			t.Errorf("unexpected call-edge kind: %v", edge["kind"])
			continue
		}
		if edge["schemaVersion"] != "1" {
			t.Errorf("unexpected schemaVersion: %v", edge["schemaVersion"])
		}
		// Both source and target CIDs must be non-null/non-empty strings.
		srcCid, hasSrc := edge["sourceContractCid"].(string)
		tgtCid, hasTgt := edge["targetContractCid"].(string)
		if hasSrc && hasTgt && srcCid != "" && tgtCid != "" {
			// targetContractCid must differ from sourceContractCid
			// (different functions have different contracts).
			if srcCid == tgtCid {
				t.Error("sourceContractCid and targetContractCid must differ for same-kit call between two distinct contracts")
			}
			// Both CIDs must have the blake3-512 prefix.
			if !strings.HasPrefix(srcCid, "blake3-512:") {
				t.Errorf("sourceContractCid missing blake3-512 prefix: %s", srcCid)
			}
			if !strings.HasPrefix(tgtCid, "blake3-512:") {
				t.Errorf("targetContractCid missing blake3-512 prefix: %s", tgtCid)
			}
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("no call-edge with both CIDs populated found; edges: %v", edgeList)
	}
}

// TestCallEdge_CgoRustKit asserts that a Go function calling C.rustFunc
// via cgo emits a call-edge with targetContractCid null and
// targetSymbol "rust-kit:rustFunc".
func TestCallEdge_CgoRustKit(t *testing.T) {
	done := capture()
	msg := json.RawMessage(mustMarshal(parseParams{Path: "demo.go", Source: cgoSource}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 11.0, Method: "parse", Params: msg}))
	resp := done()

	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}

	m := resultMap(resp)

	edgesRaw, ok := m["callEdges"]
	if !ok {
		t.Fatal("callEdges field missing from parse result")
	}
	edgeList, ok := edgesRaw.([]interface{})
	if !ok {
		t.Fatalf("callEdges not a list: %T", edgesRaw)
	}
	if len(edgeList) == 0 {
		t.Fatal("expected at least 1 call-edge for cgo call, got 0")
	}

	// Find the cgo call edge.
	var foundCgo bool
	for _, e := range edgeList {
		edge, ok := e.(map[string]interface{})
		if !ok {
			continue
		}
		if edge["kind"] != "call-edge" {
			continue
		}
		sym, hasSymbol := edge["targetSymbol"].(string)
		if !hasSymbol || sym != "rust-kit:rustFunc" {
			continue
		}
		// targetContractCid must be null (JSON null decodes to nil in
		// the map[string]interface{} round-trip).
		tgtCid, hasTgt := edge["targetContractCid"]
		if hasTgt && tgtCid != nil {
			t.Errorf("expected targetContractCid to be null for cgo edge, got %v", tgtCid)
		}
		// sourceContractCid must be a blake3-512 CID.
		srcCid, hasSrc := edge["sourceContractCid"].(string)
		if !hasSrc || !strings.HasPrefix(srcCid, "blake3-512:") {
			t.Errorf("sourceContractCid missing or wrong format: %v", edge["sourceContractCid"])
		}
		foundCgo = true
		break
	}
	if !foundCgo {
		t.Fatalf("no cgo call-edge with targetSymbol=rust-kit:rustFunc found; edges: %v", edgeList)
	}

	var castEdges []string
	for _, e := range edgeList {
		edge, ok := e.(map[string]interface{})
		if !ok {
			continue
		}
		sym, _ := edge["targetSymbol"].(string)
		if sym == "rust-kit:int64_t" {
			castEdges = append(castEdges, sym)
		}
	}
	if len(castEdges) != 0 {
		t.Fatalf("cgo type conversions must not be emitted as call edges; got %v in %v", castEdges, edgeList)
	}
}

// TestCallEdge_JCSBytesDeterministic asserts that lifting the same
// source twice produces byte-identical callEdge JSON — confirming the
// call-edge stream is content-addressed and deterministic.
func TestCallEdge_JCSBytesDeterministic(t *testing.T) {
	parseOnce := func() string {
		done := capture()
		msg := json.RawMessage(mustMarshal(parseParams{Path: "demo.go", Source: sameKitSource}))
		handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 20.0, Method: "parse", Params: msg}))
		resp := done()
		if resp.Error != nil {
			t.Fatalf("parse error: %s", resp.Error.Message)
		}
		// Re-marshal the result to get the raw JSON bytes. This
		// exercises the full marshal path, not just in-memory equality.
		b, err := json.Marshal(resp.Result)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		return string(b)
	}

	first := parseOnce()
	second := parseOnce()

	if first != second {
		t.Errorf("call-edge bytes differ between runs:\n  first:  %s\n  second: %s", first, second)
	}

	// Verify callEdges is present and non-empty in the output.
	var m map[string]interface{}
	if err := json.Unmarshal([]byte(first), &m); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	callEdgesRaw, ok := m["callEdges"]
	if !ok {
		t.Fatal("callEdges missing from result")
	}
	edgeList, ok := callEdgesRaw.([]interface{})
	if !ok {
		t.Fatalf("callEdges not a list: %T", callEdgesRaw)
	}
	if len(edgeList) == 0 {
		t.Fatal("expected non-empty callEdges for determinism check")
	}
}
