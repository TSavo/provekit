// SPDX-License-Identifier: Apache-2.0
//
// ProvekIt sugar and refuse attributes. Both are recognized at lift time by
// `provekit-walk`'s bind-lift pass (see walk_rpc.rs) and emitted as IR
// records (`library-sugar-binding-entry` and `refusal-memento`). The
// proc-macros themselves are no-ops at compile time: they return their
// annotated items unchanged. The substrate's structural meaning lives in
// the attribute paths `provekit::sugar` and `provekit::refuse`, which is
// what the lift kit pattern-matches.
//
// `provekit::sugar` carries:
//   concept = "<concept name>"
//   library = "<library tag>"
//   loss = [<dimension list>]                  (optional; empty if omitted)
//   observed_dimension = "<observation tag>"   (optional; observation bindings only)
//
// `provekit::refuse` carries:
//   surface = "<rusqlite::path::to::surface>"
//   concept = "<would-close concept name>"
//   reason = "<honest text reason>"
//   would_close_with_cluster = "<cluster constraint description>"

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn sugar(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn refuse(_args: TokenStream, item: TokenStream) -> TokenStream {
    item
}
