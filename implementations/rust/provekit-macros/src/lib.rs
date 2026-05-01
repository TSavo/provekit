// SPDX-License-Identifier: Apache-2.0
//
// provekit-macros
//
// Attribute proc-macros that give ProvekIt's Rust kit a decorator
// authoring surface. Same kit primitives as the `.invariant.rs`
// surface, different placement: the contract sits next to the
// function it constrains.
//
// Two macros:
//
//   #[provekit::contract(pre = ..., post = ..., inv = ...,
//                        out_binding = ..., name = ...)]
//       Annotates a function definition. Generates a hidden static
//       ContractRegistration that submits, via `inventory`, into a
//       distributed slice that `provekit-self-contracts` collects.
//       At least one of pre / post / inv MUST be supplied; an empty
//       contract is a compile error.
//
//   #[provekit::verify]
//       Annotates a function definition. Generates a hidden static
//       VerifyTarget marker registered via `inventory`. The build
//       script (out of scope for this crate; a follow-up task wires
//       it) walks these markers and runs the verifier against each
//       function's body.
//
// Neither macro modifies the function body. Both are purely additive:
// the attribute leaves the original `fn` in place and emits sibling
// statics under hidden names.
//
// Generated code references re-exports under
// `::provekit_macros_rt::__priv::...`. The macro expansion surface is
// stable across kit versions even if the registration types evolve.
//
// IR primitive expressions (forall / gt / num / ...) are taken
// verbatim as `syn::Expr` and embedded in the generated builder. The
// macro does NOT introduce a custom DSL; the user writes ordinary
// Rust expressions that resolve in the call-site's name resolution
// scope, exactly as they would in a `.invariant.rs` file.
//
// Conflict-resolution semantics for contracts authored via this
// surface AND the `.invariant.rs` surface live in
// protocol/specs/2026-04-30-contract-merge-semantics.md.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Expr, Ident, ItemFn, LitStr, Token};

// ---------------------------------------------------------------------------
// #[provekit::contract(...)] argument parsing
// ---------------------------------------------------------------------------

/// One `key = value` clause inside the attribute, e.g. `pre = forall(Int(), |n| gt(n, num(0)))`.
struct ContractArg {
    key: Ident,
    _eq: Token![=],
    value: Expr,
}

impl Parse for ContractArg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            key: input.parse()?,
            _eq: input.parse()?,
            value: input.parse()?,
        })
    }
}

/// Holds the parsed attribute body. We accept four optional formula
/// slots (pre / post / inv / out_binding) plus an optional name
/// override. Slots default to None / the function's own ident.
#[derive(Default)]
struct ContractArgs {
    pre: Option<Expr>,
    post: Option<Expr>,
    inv: Option<Expr>,
    out_binding: Option<Expr>,
    name: Option<Expr>,
}

impl ContractArgs {
    fn from_attr(input: TokenStream2) -> syn::Result<Self> {
        // Empty attribute body is a hard error; the protocol requires
        // at least one of pre/post/inv to be present, and we'd rather
        // fail at attribute parse than at the (also-checked) downstream
        // mint step.
        if input.is_empty() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[provekit::contract] requires at least one of pre / post / inv",
            ));
        }

        let parser = Punctuated::<ContractArg, Token![,]>::parse_terminated;
        let parsed = syn::parse::Parser::parse2(parser, input)?;

        let mut out = Self::default();
        for kv in parsed {
            let k = kv.key.to_string();
            match k.as_str() {
                "pre" => {
                    if out.pre.is_some() {
                        return Err(syn::Error::new_spanned(kv.key, "duplicate `pre` argument"));
                    }
                    out.pre = Some(kv.value);
                }
                "post" => {
                    if out.post.is_some() {
                        return Err(syn::Error::new_spanned(kv.key, "duplicate `post` argument"));
                    }
                    out.post = Some(kv.value);
                }
                "inv" => {
                    if out.inv.is_some() {
                        return Err(syn::Error::new_spanned(kv.key, "duplicate `inv` argument"));
                    }
                    out.inv = Some(kv.value);
                }
                "out_binding" => {
                    if out.out_binding.is_some() {
                        return Err(syn::Error::new_spanned(
                            kv.key,
                            "duplicate `out_binding` argument",
                        ));
                    }
                    out.out_binding = Some(kv.value);
                }
                "name" => {
                    if out.name.is_some() {
                        return Err(syn::Error::new_spanned(kv.key, "duplicate `name` argument"));
                    }
                    out.name = Some(kv.value);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        kv.key,
                        format!(
                            "unknown #[provekit::contract] argument `{other}`; \
                             expected one of: pre, post, inv, out_binding, name"
                        ),
                    ));
                }
            }
        }

        if out.pre.is_none() && out.post.is_none() && out.inv.is_none() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "#[provekit::contract] requires at least one of pre / post / inv",
            ));
        }

        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// #[provekit::contract(...)] expansion
// ---------------------------------------------------------------------------

#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match ContractArgs::from_attr(attr.into()) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let item_fn = parse_macro_input!(item as ItemFn);
    let fn_ident = item_fn.sig.ident.clone();

    // Per task spec: name defaults to the function's identifier; the
    // `name = "..."` argument overrides it. `name` accepts any Expr
    // that's `Into<String>` so authors can use a const, a string
    // literal, or any other expression resolvable at call site.
    let fn_name_str = fn_ident.to_string();
    let name_expr = match args.name {
        Some(e) => quote! { #e },
        None => {
            let lit = LitStr::new(&fn_name_str, fn_ident.span());
            quote! { #lit }
        }
    };

    // Build the formula expressions. Each Option<Expr> becomes either
    // `None` or `Some({user expr})` in the generated builder. We do
    // NOT canonicalize here; the kit's `forall` / `gt` / `num` / ...
    // primitives already return `Rc<Formula>` / `Rc<Term>`.
    let pre_tokens = match &args.pre {
        Some(e) => quote! { Some(#e) },
        None => quote! { None },
    };
    let post_tokens = match &args.post {
        Some(e) => quote! { Some(#e) },
        None => quote! { None },
    };
    let inv_tokens = match &args.inv {
        Some(e) => quote! { Some(#e) },
        None => quote! { None },
    };
    let out_binding_tokens = match &args.out_binding {
        Some(e) => quote! { Into::<String>::into(#e) },
        None => quote! { String::from("out") },
    };

    // Hidden static name. Suffix with the function ident so multiple
    // `#[contract]` annotations in the same module don't collide.
    let static_ident = quote::format_ident!("__PROVEKIT_CONTRACT_{}", fn_ident);
    let builder_ident = quote::format_ident!("__provekit_contract_builder_{}", fn_ident);

    let expanded = quote! {
        #item_fn

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #builder_ident() -> ::provekit_macros_rt::__priv::provekit_ir_symbolic::ContractDecl {
            // Resolve the user-supplied expressions in the call
            // site's scope. They must produce Rc<Formula> for the
            // pre/post/inv slots; the kit's `forall` / `gt` / etc.
            // do exactly that.
            let __pre = #pre_tokens;
            let __post = #post_tokens;
            let __inv = #inv_tokens;
            let __out_binding: String = #out_binding_tokens;
            let __name: String = (#name_expr).to_string();
            ::provekit_macros_rt::__priv::provekit_ir_symbolic::ContractDecl {
                name: __name,
                pre: __pre,
                post: __post,
                inv: __inv,
                out_binding: __out_binding,
            }
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals, non_snake_case)]
        static #static_ident: ::provekit_macros_rt::__priv::ContractRegistration =
            ::provekit_macros_rt::__priv::ContractRegistration {
                name: #fn_name_str,
                source_path: file!(),
                source_line: line!(),
                builder: #builder_ident,
            };

        ::provekit_macros_rt::__priv::inventory::submit! {
            ::provekit_macros_rt::__priv::ContractRegistration {
                name: #fn_name_str,
                source_path: file!(),
                source_line: line!(),
                builder: #builder_ident,
            }
        }
    };

    expanded.into()
}

// ---------------------------------------------------------------------------
// #[provekit::verify] expansion
// ---------------------------------------------------------------------------

#[proc_macro_attribute]
pub fn verify(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        let err = syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[provekit::verify] takes no arguments",
        );
        return err.to_compile_error().into();
    }

    let item_fn = parse_macro_input!(item as ItemFn);
    let fn_ident = item_fn.sig.ident.clone();
    let fn_name_str = fn_ident.to_string();

    // Coarse "did this body change?" hint. We hash the token stream
    // of the whole `fn` (signature + body) at expansion time. This is
    // NOT the protocol's canonical AST hash; the lifter computes that
    // separately in a build-script step that lives in a follow-up
    // task. The hint here is purely local and exists so cached
    // call-site-enumeration tables can detect staleness without
    // re-running the lifter on every build.
    //
    // TODO(follow-up): wire the build script that walks
    // `inventory::iter::<VerifyTarget>` and dispatches each function's
    // body against the verifier.
    let mut hasher: u64 = 1469598103934665603u64; // FNV-1a 64-bit offset basis
    for byte in item_fn
        .clone()
        .into_token_stream_string()
        .as_bytes()
    {
        hasher ^= *byte as u64;
        hasher = hasher.wrapping_mul(1099511628211u64); // FNV-1a 64-bit prime
    }
    let hash_lit = hasher;

    let static_ident = quote::format_ident!("__PROVEKIT_VERIFY_{}", fn_ident);

    let expanded = quote! {
        #item_fn

        #[doc(hidden)]
        #[allow(non_upper_case_globals, non_snake_case)]
        static #static_ident: ::provekit_macros_rt::__priv::VerifyTarget =
            ::provekit_macros_rt::__priv::VerifyTarget {
                fn_name: #fn_name_str,
                source_path: file!(),
                source_line: line!(),
                ast_hash_hint: #hash_lit,
            };

        ::provekit_macros_rt::__priv::inventory::submit! {
            ::provekit_macros_rt::__priv::VerifyTarget {
                fn_name: #fn_name_str,
                source_path: file!(),
                source_line: line!(),
                ast_hash_hint: #hash_lit,
            }
        }
    };

    expanded.into()
}

// ---------------------------------------------------------------------------
// Helper trait: serialize an ItemFn's tokens to a string for hashing.
// ---------------------------------------------------------------------------

trait TokenStreamString {
    fn into_token_stream_string(self) -> String;
}

impl<T: quote::ToTokens> TokenStreamString for T {
    fn into_token_stream_string(self) -> String {
        let mut ts = TokenStream2::new();
        self.to_tokens(&mut ts);
        ts.to_string()
    }
}
