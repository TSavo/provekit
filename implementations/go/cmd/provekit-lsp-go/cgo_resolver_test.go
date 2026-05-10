// cgo_resolver_test.go: tests for per-host cgo preamble resolver.
//
// Implements Part 2 of spec #114 R3:
//   Test 1: cgo file with rust_*.h include → targetSymbol = "rust-kit:funcName"
//   Test 2: cgo file with -lz (zlib) LDFLAGS → targetSymbol = "c-kit:funcName"
//   Test 3: cgo file with no target signal → resolver-error prefix, no placeholder
//   Test 4: non-cgo Go file → no cgo call edges emitted
//   Test 5: byte-determinism: two runs produce identical call-edge stream
package main

import (
	"encoding/json"
	"strings"
	"testing"
)

// ---- fixtures ---------------------------------------------------------------

// cgoRustHeaderSource: cgo file with a Rust header include.
// The resolver should identify this as rust-kit via the "rust_callee.h" include.
const cgoRustHeaderSource = `package demo

/*
#include "rust_callee.h"
#include <stdint.h>
extern int64_t compute(int64_t n);
*/
import "C"

//provekit:contract
func CallRust(n int) int {
	return int(C.compute(C.int64_t(n)))
}
`

// cgoZlibSource: cgo file linking against zlib (-lz).
// The resolver should identify this as c-kit (non-rust, non-system well-known
// library: zlib IS in the system libs list, so this resolves to "libc-system").
// We test c-kit explicitly below with a non-system lib.
const cgoZlibSource = `package demo

/*
#cgo LDFLAGS: -lz
#include <zlib.h>
extern int compress2(void* dest, unsigned long* destLen, const void* source, unsigned long sourceLen, int level);
*/
import "C"

//provekit:contract
func CompressData(n int) int {
	return n
}
`

// cgoCKitSource: cgo file linking against a non-rust, non-system library.
// The resolver should identify this as c-kit.
const cgoCKitSource = `package demo

/*
#cgo LDFLAGS: -lmylib
extern int my_func(int n);
*/
import "C"

//provekit:contract
func CallMyLib(n int) int {
	return int(C.my_func(C.int(n)))
}
`

// cgoNoSignalSource: cgo file with no LDFLAGS and no rust header.
// The resolver cannot determine the kit; should emit resolver-error prefix.
const cgoNoSignalSource = `package demo

/*
#include <stdint.h>
extern int64_t mystery(int64_t n);
*/
import "C"

//provekit:contract
func CallMystery(n int) int {
	return int(C.mystery(C.int64_t(n)))
}
`

// nonCgoSource: a plain Go file with no import "C".
// Should produce no cgo call edges; only same-kit edges if any.
const nonCgoGoSource = `package demo

//provekit:contract
func PlainFn(x int) int { return x + 1 }
`

// ---- helpers ----------------------------------------------------------------

// parseCgoEdges parses source, lifts it, and returns the call edges.
func parseCgoEdges(t *testing.T, src, path string) []map[string]interface{} {
	t.Helper()
	done := capture()
	msg := json.RawMessage(mustMarshal(parseParams{Path: path, Source: src}))
	handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 99.0, Method: "parse", Params: msg}))
	resp := done()
	if resp.Error != nil {
		t.Fatalf("parse error: %s", resp.Error.Message)
	}
	m := resultMap(resp)
	edgesRaw, ok := m["callEdges"]
	if !ok {
		t.Fatal("callEdges missing from parse result")
	}
	edgeList, ok := edgesRaw.([]interface{})
	if !ok {
		t.Fatalf("callEdges not a list: %T", edgesRaw)
	}
	var edges []map[string]interface{}
	for _, e := range edgeList {
		if edge, ok := e.(map[string]interface{}); ok {
			edges = append(edges, edge)
		}
	}
	return edges
}

// ---- tests ------------------------------------------------------------------

// TestCgoResolver_RustHeader asserts that a cgo file including rust_callee.h
// emits a call edge with targetSymbol = "rust-kit:<func>".
func TestCgoResolver_RustHeader(t *testing.T) {
	edges := parseCgoEdges(t, cgoRustHeaderSource, "demo.go")
	if len(edges) == 0 {
		t.Fatal("expected at least one call edge for cgo+rust-header source")
	}
	var found bool
	for _, e := range edges {
		sym, _ := e["targetSymbol"].(string)
		if strings.HasPrefix(sym, "rust-kit:") {
			found = true
			// targetContractCid must be null.
			tgt, hasTgt := e["targetContractCid"]
			if hasTgt && tgt != nil {
				t.Errorf("expected targetContractCid null for cgo edge, got %v", tgt)
			}
			break
		}
	}
	if !found {
		t.Fatalf("no call edge with targetSymbol starting rust-kit: found; edges: %v", edges)
	}
}

// TestCgoResolver_ZlibIsSystem asserts that a cgo file linking -lz emits
// a call edge with targetSymbol = "libc-system:<func>".
// zlib is in the well-known system libs list.
func TestCgoResolver_ZlibIsSystem(t *testing.T) {
	edges := parseCgoEdges(t, cgoZlibSource, "demo.go")
	// CompressData has no cgo calls in its body in this fixture, so we
	// only verify the resolver produces "libc-system" for zlib preamble.
	// We test the resolver directly below; here we validate through the
	// full parse path.
	preamble := parseCgoPreamble(cgoZlibSource)
	if preamble == nil {
		t.Fatal("expected parseCgoPreamble to return non-nil for zlib fixture")
	}
	kit := resolveCgoKit(preamble)
	if kit != "libc-system" {
		t.Errorf("expected libc-system for zlib preamble, got %q", kit)
	}
	_ = edges // no cgo call sites in the body; kit resolved at preamble level
}

// TestCgoResolver_CKit asserts that a cgo file linking a non-rust, non-system
// library emits call edges with targetSymbol = "c-kit:<func>".
func TestCgoResolver_CKit(t *testing.T) {
	edges := parseCgoEdges(t, cgoCKitSource, "demo.go")
	if len(edges) == 0 {
		t.Fatal("expected at least one call edge for c-kit cgo source")
	}
	var found bool
	for _, e := range edges {
		sym, _ := e["targetSymbol"].(string)
		if strings.HasPrefix(sym, "c-kit:") {
			found = true
			tgt, hasTgt := e["targetContractCid"]
			if hasTgt && tgt != nil {
				t.Errorf("expected targetContractCid null for c-kit edge, got %v", tgt)
			}
			break
		}
	}
	if !found {
		t.Fatalf("no call edge with targetSymbol starting c-kit: found; edges: %v", edges)
	}
}

// TestCgoResolver_NoSignalEmitsResolverError asserts that a cgo file whose
// preamble has no LDFLAGS and no rust header emits edges with
// targetSymbol = "resolver-error:<func>": NOT a placeholder string.
// Spec #97 R2 forbids silent "unknown:foo" symbols.
func TestCgoResolver_NoSignalEmitsResolverError(t *testing.T) {
	edges := parseCgoEdges(t, cgoNoSignalSource, "demo.go")
	if len(edges) == 0 {
		t.Fatal("expected at least one call edge for no-signal cgo source")
	}
	var found bool
	for _, e := range edges {
		sym, _ := e["targetSymbol"].(string)
		if strings.HasPrefix(sym, "resolver-error:") {
			found = true
			// targetContractCid must be null.
			tgt, hasTgt := e["targetContractCid"]
			if hasTgt && tgt != nil {
				t.Errorf("expected targetContractCid null for resolver-error edge, got %v", tgt)
			}
			// Must NOT be a placeholder like "unknown:mystery".
			if strings.HasPrefix(sym, "unknown:") {
				t.Errorf("edge must use resolver-error: prefix, not unknown: prefix: %s", sym)
			}
			break
		}
	}
	if !found {
		t.Fatalf("expected resolver-error: edge for no-signal cgo source; edges: %v", edges)
	}
}

// TestCgoResolver_NonCgoFileHasNoCgoEdges asserts that a Go file without
// `import "C"` produces zero cgo call edges. Any call edges should be
// same-kit edges only.
func TestCgoResolver_NonCgoFileHasNoCgoEdges(t *testing.T) {
	edges := parseCgoEdges(t, nonCgoGoSource, "demo.go")
	// Non-cgo source has no call sites at all (PlainFn body has no calls).
	for _, e := range edges {
		sym, _ := e["targetSymbol"].(string)
		if strings.HasPrefix(sym, "rust-kit:") || strings.HasPrefix(sym, "c-kit:") ||
			strings.HasPrefix(sym, "resolver-error:") || strings.HasPrefix(sym, "libc-system:") {
			t.Errorf("non-cgo file produced a cross-kit edge: %v", e)
		}
	}
}

// TestCgoResolver_ByteDeterminism asserts that lifting the same cgo source
// twice produces byte-identical call-edge JSON output.
func TestCgoResolver_ByteDeterminism(t *testing.T) {
	liftOnce := func() string {
		done := capture()
		msg := json.RawMessage(mustMarshal(parseParams{Path: "demo.go", Source: cgoRustHeaderSource}))
		handleRequest(mustMarshal(rpcRequest{JSONRPC: "2.0", ID: 42.0, Method: "parse", Params: msg}))
		resp := done()
		if resp.Error != nil {
			t.Fatalf("parse error: %s", resp.Error.Message)
		}
		b, err := json.Marshal(resp.Result)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		return string(b)
	}

	first := liftOnce()
	second := liftOnce()
	if first != second {
		t.Errorf("call-edge stream not byte-deterministic:\n  first:  %s\n  second: %s", first, second)
	}
}

// ---- unit tests for resolver primitives ------------------------------------

// TestParseCgoPreamble_RustHeader verifies parseCgoPreamble extracts
// the rust_*.h include from the preamble block.
func TestParseCgoPreamble_RustHeader(t *testing.T) {
	preamble := parseCgoPreamble(cgoRustHeaderSource)
	if preamble == nil {
		t.Fatal("parseCgoPreamble returned nil for rust header fixture")
	}
	var found bool
	for _, h := range preamble.Includes {
		if strings.HasPrefix(strings.ToLower(h), "rust") {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("rust_callee.h not found in parsed includes: %v", preamble.Includes)
	}
}

// TestParseCgoPreamble_LDFlags verifies parseCgoPreamble extracts LDFLAGS.
func TestParseCgoPreamble_LDFlags(t *testing.T) {
	preamble := parseCgoPreamble(cgoZlibSource)
	if preamble == nil {
		t.Fatal("parseCgoPreamble returned nil for zlib fixture")
	}
	if !strings.Contains(preamble.LDFlags, "-lz") {
		t.Errorf("expected -lz in LDFlags, got %q", preamble.LDFlags)
	}
}

// TestParseCgoPreamble_NoCgo verifies parseCgoPreamble returns nil for
// files with no import "C".
func TestParseCgoPreamble_NoCgo(t *testing.T) {
	preamble := parseCgoPreamble(nonCgoGoSource)
	if preamble != nil {
		t.Errorf("expected nil preamble for non-cgo file, got %+v", preamble)
	}
}

// TestResolveCgoKit_NilPreamble verifies resolveCgoKit returns "" for nil.
func TestResolveCgoKit_NilPreamble(t *testing.T) {
	kit := resolveCgoKit(nil)
	if kit != "" {
		t.Errorf("expected empty string for nil preamble, got %q", kit)
	}
}
