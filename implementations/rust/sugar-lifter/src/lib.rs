// SPDX-License-Identifier: Apache-2.0
//
// ProvekIt sugar, boundary, and refuse attributes. All three are recognized
// at lift time by `provekit-walk`'s bind-lift pass (see walk_rpc.rs) and
// emitted as IR records:
//   sugar    → `library-sugar-binding-entry`     (this code IS a concept materialization)
//   boundary → `realization-memento` (Boundary)  (this code is the EDGE where the concept binds to a library)
//   refuse   → `refusal-memento`                 (this surface is declined)
// The proc-macros themselves are no-ops at compile time: they return their
// annotated items unchanged. The substrate's structural meaning lives in
// the attribute paths, which is what the lift kit pattern-matches.
//
// `provekit::sugar` carries:
//   concept = "<concept name>"
//   library = "<library tag>"
//   loss = [<dimension list>]                  (optional; empty if omitted)
//   observed_dimension = "<observation tag>"   (optional; observation bindings only)
//
// `provekit::boundary` carries:
//   concept = "<concept name>"                 the shared contract identity
//   library = "<source-language library>"     this source's library (e.g., "blake3")
//   api = "<symbol or surface path>"           optional: specific API binding
//   boundary_contract = "<boundary:* name>"    optional: catalog entry CID-keyed
//   loss = [<dimension list>]                  (optional; empty if omitted)
//
// `provekit::refuse` carries:
//   surface = "<path::to::surface>"
//   concept = "<would-close concept name>"
//   reason = "<honest text reason>"
//   would_close_with_cluster = "<cluster constraint description>"
//
// SUGAR vs BOUNDARY (the typology):
//
// Sugar marks the LIFTING direction: "this source code IS a materialization
// of concept X." It feeds the lifter — the substrate climbs UP through the
// concept hub via sugar annotations.
//
// Boundary marks the LOWERING-EDGE direction: "this callsite is WHERE the
// materialization of concept X stops descending and binds to a library —
// substitute the per-target library here." It feeds the materializer — when
// realizing a downstream consumer in a different language, the materializer
// reads @boundary annotations and picks the per-target sister library
// (Rust's blake3 → TS's @noble/hashes/blake3; Rust's std::io → TS's
// node:readline). Boundary is what makes cross-language realization
// tractable: the substrate doesn't translate library source code; it
// substitutes per-language libraries at each boundary.
//
// Same function can carry both: it IS a materialization of the concept
// (@sugar) AND it IS the edge of a library boundary (@boundary). The two
// annotations name different roles even when co-located.

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn sugar(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn boundary(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn refuse(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}
