// SPDX-License-Identifier: Apache-2.0
//
// Type declarations: enums, structs, traits, impl blocks.
//
// Per #377. Each type definition emits a content-addressed memento
// describing the type's shape; impl blocks emit one
// FunctionContractMemento per method linked back to the type via
// `parent_type_cid`. Trait definitions emit method-signature
// mementos that bind impl methods can satisfy.
//
// Schema overview:
//
//   TypeDeclMemento (struct):
//     { schemaVersion: "1", kind: "type-decl-struct",
//       name: <string>, fields: [{name, sort}, ...], locus }
//
//   TypeDeclMemento (enum):
//     { schemaVersion: "1", kind: "type-decl-enum",
//       name: <string>, variants: [VariantMemento, ...], locus }
//
//   VariantMemento:
//     { name: <string>, kind: "unit" | "tuple" | "struct",
//       fields: [<sort>, ...] | [{name, sort}, ...] | [] }
//
//   TraitMemento:
//     { schemaVersion: "1", kind: "trait",
//       name: <string>, method_signatures: [<MethodSignature>, ...], locus }
//
//   MethodSignatureMemento:
//     { schemaVersion: "1", kind: "method-signature",
//       trait_name: <string>, method_name: <string>,
//       formals, formal_sorts, return_sort, locus }
//
//   ImplMemento:
//     { schemaVersion: "1", kind: "impl",
//       target_type: <string>, trait_name: <string or null>,
//       method_cids: [<cid>, ...], locus }
//
// MVP scope: extract these from a syn::File and emit
// content-addressed bytes. Substrate-side resolution (binding impl
// methods to trait method signatures, etc.) is downstream.

use std::sync::Arc;

use provekit_canonicalizer::Value;
use provekit_ir_types::Sort;
use syn::{File, Item, ItemEnum, ItemImpl, ItemStruct, ItemTrait};

use crate::canonical::{cid_of_value, jcs_bytes_of_value};
use crate::contract::build_function_contract_with_file;
use crate::locus::Locus;

// ---- Struct ----

#[derive(Debug, Clone)]
pub struct StructDeclMemento {
    pub name: String,
    pub fields: Vec<(String, Sort)>,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

pub fn lift_struct_decl(item: &ItemStruct, file_path: Option<&str>) -> StructDeclMemento {
    let name = item.ident.to_string();
    let fields = match &item.fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let field_name = f
                    .ident
                    .as_ref()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "<unnamed>".to_string());
                (field_name, infer_sort(&f.ty))
            })
            .collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| (format!("{}", i), infer_sort(&f.ty)))
            .collect(),
        syn::Fields::Unit => vec![],
    };
    let locus = Locus::from_span(item.ident.span(), file_path);

    let fields_arr: Vec<Arc<Value>> = fields
        .iter()
        .map(|(n, s)| {
            Value::object([
                ("name", Value::string(n.clone())),
                ("sort", sort_to_value(s)),
            ])
        })
        .collect();
    let value = Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("type-decl-struct")),
        ("name", Value::string(name.clone())),
        ("fields", Value::array(fields_arr)),
        ("locus", locus.to_value()),
    ]);
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    StructDeclMemento {
        name,
        fields,
        locus,
        canonical_bytes,
        cid,
    }
}

// ---- Enum ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantShape {
    Unit,
    Tuple(Vec<Sort>),
    Struct(Vec<(String, Sort)>),
}

#[derive(Debug, Clone)]
pub struct VariantMemento {
    pub name: String,
    pub shape: VariantShape,
}

impl VariantMemento {
    fn to_value(&self) -> Arc<Value> {
        match &self.shape {
            VariantShape::Unit => Value::object([
                ("name", Value::string(self.name.clone())),
                ("kind", Value::string("unit")),
            ]),
            VariantShape::Tuple(sorts) => {
                let arr: Vec<Arc<Value>> = sorts.iter().map(sort_to_value).collect();
                Value::object([
                    ("name", Value::string(self.name.clone())),
                    ("kind", Value::string("tuple")),
                    ("fields", Value::array(arr)),
                ])
            }
            VariantShape::Struct(fields) => {
                let arr: Vec<Arc<Value>> = fields
                    .iter()
                    .map(|(n, s)| {
                        Value::object([
                            ("name", Value::string(n.clone())),
                            ("sort", sort_to_value(s)),
                        ])
                    })
                    .collect();
                Value::object([
                    ("name", Value::string(self.name.clone())),
                    ("kind", Value::string("struct")),
                    ("fields", Value::array(arr)),
                ])
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EnumDeclMemento {
    pub name: String,
    pub variants: Vec<VariantMemento>,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

pub fn lift_enum_decl(item: &ItemEnum, file_path: Option<&str>) -> EnumDeclMemento {
    let name = item.ident.to_string();
    let variants: Vec<VariantMemento> = item
        .variants
        .iter()
        .map(|v| {
            let vname = v.ident.to_string();
            let shape = match &v.fields {
                syn::Fields::Unit => VariantShape::Unit,
                syn::Fields::Unnamed(unnamed) => {
                    VariantShape::Tuple(unnamed.unnamed.iter().map(|f| infer_sort(&f.ty)).collect())
                }
                syn::Fields::Named(named) => VariantShape::Struct(
                    named
                        .named
                        .iter()
                        .map(|f| {
                            let field_name = f
                                .ident
                                .as_ref()
                                .map(|i| i.to_string())
                                .unwrap_or_default();
                            (field_name, infer_sort(&f.ty))
                        })
                        .collect(),
                ),
            };
            VariantMemento { name: vname, shape }
        })
        .collect();
    let locus = Locus::from_span(item.ident.span(), file_path);

    let variants_arr: Vec<Arc<Value>> = variants.iter().map(|v| v.to_value()).collect();
    let value = Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("type-decl-enum")),
        ("name", Value::string(name.clone())),
        ("variants", Value::array(variants_arr)),
        ("locus", locus.to_value()),
    ]);
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    EnumDeclMemento {
        name,
        variants,
        locus,
        canonical_bytes,
        cid,
    }
}

// ---- Trait ----

#[derive(Debug, Clone)]
pub struct MethodSignatureMemento {
    pub trait_name: String,
    pub method_name: String,
    pub formals: Vec<String>,
    pub formal_sorts: Vec<Sort>,
    pub return_sort: Sort,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

#[derive(Debug, Clone)]
pub struct TraitMemento {
    pub name: String,
    pub method_signature_cids: Vec<String>,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

pub fn lift_trait_decl(
    item: &ItemTrait,
    file_path: Option<&str>,
) -> (TraitMemento, Vec<MethodSignatureMemento>) {
    let trait_name = item.ident.to_string();
    let mut signatures = Vec::new();
    for trait_item in &item.items {
        if let syn::TraitItem::Fn(method) = trait_item {
            let method_name = method.sig.ident.to_string();
            let (formals, formal_sorts) = extract_formals_from_sig(&method.sig);
            let return_sort = match &method.sig.output {
                syn::ReturnType::Default => Sort::Primitive {
                    name: "Unit".to_string(),
                },
                syn::ReturnType::Type(_, ty) => infer_sort(ty),
            };
            let locus = Locus::from_span(method.sig.ident.span(), file_path);

            let formals_arr: Vec<Arc<Value>> =
                formals.iter().map(|n| Value::string(n.clone())).collect();
            let formal_sorts_arr: Vec<Arc<Value>> =
                formal_sorts.iter().map(sort_to_value).collect();
            let value = Value::object([
                ("schemaVersion", Value::string("1")),
                ("kind", Value::string("method-signature")),
                ("traitName", Value::string(trait_name.clone())),
                ("methodName", Value::string(method_name.clone())),
                ("formals", Value::array(formals_arr)),
                ("formalSorts", Value::array(formal_sorts_arr)),
                ("returnSort", sort_to_value(&return_sort)),
                ("locus", locus.to_value()),
            ]);
            let canonical_bytes = jcs_bytes_of_value(&value);
            let cid = cid_of_value(&value);

            signatures.push(MethodSignatureMemento {
                trait_name: trait_name.clone(),
                method_name,
                formals,
                formal_sorts,
                return_sort,
                locus,
                canonical_bytes,
                cid,
            });
        }
    }
    let trait_locus = Locus::from_span(item.ident.span(), file_path);
    let sig_cids: Vec<Arc<Value>> = signatures
        .iter()
        .map(|s| Value::string(s.cid.clone()))
        .collect();
    let trait_value = Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("trait")),
        ("name", Value::string(trait_name.clone())),
        ("methodSignatures", Value::array(sig_cids.clone())),
        ("locus", trait_locus.to_value()),
    ]);
    let trait_bytes = jcs_bytes_of_value(&trait_value);
    let trait_cid = cid_of_value(&trait_value);

    (
        TraitMemento {
            name: trait_name,
            method_signature_cids: signatures.iter().map(|s| s.cid.clone()).collect(),
            locus: trait_locus,
            canonical_bytes: trait_bytes,
            cid: trait_cid,
        },
        signatures,
    )
}

// ---- Impl block ----

#[derive(Debug, Clone)]
pub struct ImplMemento {
    pub target_type: String,
    pub trait_name: Option<String>,
    pub method_cids: Vec<String>,
    pub locus: Locus,
    pub canonical_bytes: Vec<u8>,
    pub cid: String,
}

pub fn lift_impl_block(
    item: &ItemImpl,
    file_path: Option<&str>,
) -> (
    ImplMemento,
    Vec<crate::contract::FunctionContractMemento>,
) {
    let target_type = type_name(&item.self_ty);
    let trait_name = item.trait_.as_ref().map(|(_, path, _)| {
        path.segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default()
    });

    let mut methods = Vec::new();
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            let synthetic = syn::ItemFn {
                attrs: method.attrs.clone(),
                vis: method.vis.clone(),
                sig: method.sig.clone(),
                block: Box::new(method.block.clone()),
            };
            let contract = build_function_contract_with_file(&synthetic, None, file_path);
            methods.push(contract);
        }
    }

    let locus = Locus::from_span(item.impl_token.span, file_path);
    let method_cids: Vec<String> = methods.iter().map(|m| m.cid.clone()).collect();
    let method_cids_arr: Vec<Arc<Value>> = method_cids
        .iter()
        .map(|c| Value::string(c.clone()))
        .collect();
    let trait_value: Arc<Value> = match &trait_name {
        Some(t) => Value::string(t.clone()),
        None => Value::null(),
    };
    let value = Value::object([
        ("schemaVersion", Value::string("1")),
        ("kind", Value::string("impl")),
        ("targetType", Value::string(target_type.clone())),
        ("traitName", trait_value),
        ("methodCids", Value::array(method_cids_arr)),
        ("locus", locus.to_value()),
    ]);
    let canonical_bytes = jcs_bytes_of_value(&value);
    let cid = cid_of_value(&value);

    (
        ImplMemento {
            target_type,
            trait_name,
            method_cids,
            locus,
            canonical_bytes,
            cid,
        },
        methods,
    )
}

// ---- File-level dispatch ----

#[derive(Debug, Clone, Default)]
pub struct TypeDeclSet {
    pub structs: Vec<StructDeclMemento>,
    pub enums: Vec<EnumDeclMemento>,
    pub traits: Vec<TraitMemento>,
    pub method_signatures: Vec<MethodSignatureMemento>,
    pub impls: Vec<ImplMemento>,
    pub impl_methods: Vec<crate::contract::FunctionContractMemento>,
}

/// Walk a syn::File and extract every type declaration + impl block.
pub fn lift_file_type_decls(file: &File, file_path: Option<&str>) -> TypeDeclSet {
    let mut out = TypeDeclSet::default();
    for item in &file.items {
        match item {
            Item::Struct(s) => out.structs.push(lift_struct_decl(s, file_path)),
            Item::Enum(e) => out.enums.push(lift_enum_decl(e, file_path)),
            Item::Trait(t) => {
                let (trait_memento, sigs) = lift_trait_decl(t, file_path);
                out.traits.push(trait_memento);
                out.method_signatures.extend(sigs);
            }
            Item::Impl(i) => {
                let (impl_memento, methods) = lift_impl_block(i, file_path);
                out.impls.push(impl_memento);
                out.impl_methods.extend(methods);
            }
            _ => {}
        }
    }
    out
}

// ---- helpers ----

fn extract_formals_from_sig(sig: &syn::Signature) -> (Vec<String>, Vec<Sort>) {
    let mut names = Vec::new();
    let mut sorts = Vec::new();
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(_) => {
                names.push("self".to_string());
                sorts.push(Sort::Primitive {
                    name: "Self".to_string(),
                });
            }
            syn::FnArg::Typed(pt) => {
                let n = match &*pt.pat {
                    syn::Pat::Ident(p) => p.ident.to_string(),
                    _ => "<arg>".to_string(),
                };
                names.push(n);
                sorts.push(infer_sort(&pt.ty));
            }
        }
    }
    (names, sorts)
}

fn type_name(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string().replace(' ', "")
}

fn infer_sort(ty: &syn::Type) -> Sort {
    use quote::ToTokens;
    let s = ty.to_token_stream().to_string();
    let name = match s.trim() {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
        | "u128" | "usize" => "Int",
        "bool" => "Bool",
        "f32" | "f64" => "Real",
        "String" | "& str" | "&str" => "String",
        _ => "Int",
    };
    Sort::Primitive {
        name: name.to_string(),
    }
}

fn sort_to_value(s: &Sort) -> Arc<Value> {
    match s {
        Sort::Primitive { name } => Value::object([
            ("kind", Value::string("primitive")),
            ("name", Value::string(name.clone())),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_file(src: &str) -> File {
        syn::parse_str(src).unwrap()
    }

    #[test]
    fn struct_decl_lifts_fields_with_sorts() {
        let f = parse_file(
            r#"
            struct Point { x: u32, y: u32 }
        "#,
        );
        let set = lift_file_type_decls(&f, Some("test.rs"));
        assert_eq!(set.structs.len(), 1);
        let p = &set.structs[0];
        assert_eq!(p.name, "Point");
        assert_eq!(p.fields.len(), 2);
        assert_eq!(p.fields[0].0, "x");
        assert_eq!(p.fields[1].0, "y");
        assert!(p.cid.starts_with("blake3-512:"));
    }

    #[test]
    fn enum_decl_lifts_variants_with_shapes() {
        let f = parse_file(
            r#"
            enum Color {
                Red,
                Green,
                Blue(u32),
                Custom { r: u32, g: u32, b: u32 },
            }
        "#,
        );
        let set = lift_file_type_decls(&f, Some("test.rs"));
        assert_eq!(set.enums.len(), 1);
        let c = &set.enums[0];
        assert_eq!(c.name, "Color");
        assert_eq!(c.variants.len(), 4);
        assert!(matches!(c.variants[0].shape, VariantShape::Unit));
        assert!(matches!(c.variants[2].shape, VariantShape::Tuple(_)));
        assert!(matches!(c.variants[3].shape, VariantShape::Struct(_)));
    }

    #[test]
    fn trait_decl_emits_method_signatures() {
        let f = parse_file(
            r#"
            trait Greet {
                fn hello(&self) -> u32;
                fn hello_with(&self, who: u32) -> u32;
            }
        "#,
        );
        let set = lift_file_type_decls(&f, Some("test.rs"));
        assert_eq!(set.traits.len(), 1);
        let t = &set.traits[0];
        assert_eq!(t.name, "Greet");
        assert_eq!(t.method_signature_cids.len(), 2);
        assert_eq!(set.method_signatures.len(), 2);
        assert_eq!(set.method_signatures[1].method_name, "hello_with");
        assert_eq!(set.method_signatures[1].formals.len(), 2); // self + who
    }

    #[test]
    fn impl_block_emits_method_contracts() {
        let f = parse_file(
            r#"
            struct Counter { n: u32 }
            impl Counter {
                fn double(self) -> u32 {
                    self.n * 2
                }
                fn add(&self, x: u32) -> u32 {
                    self.n + x
                }
            }
        "#,
        );
        let set = lift_file_type_decls(&f, Some("test.rs"));
        assert_eq!(set.impls.len(), 1);
        let i = &set.impls[0];
        assert_eq!(i.target_type, "Counter");
        assert!(i.trait_name.is_none());
        assert_eq!(i.method_cids.len(), 2);
        assert_eq!(set.impl_methods.len(), 2);
        // Each impl method emits a function contract memento.
        assert_eq!(set.impl_methods[0].fn_name, "double");
        assert_eq!(set.impl_methods[1].fn_name, "add");
    }

    #[test]
    fn impl_for_trait_records_trait_name() {
        let f = parse_file(
            r#"
            trait Greet { fn hello(&self) -> u32; }
            struct Foo;
            impl Greet for Foo {
                fn hello(&self) -> u32 { 42 }
            }
        "#,
        );
        let set = lift_file_type_decls(&f, Some("test.rs"));
        assert_eq!(set.impls.len(), 1);
        assert_eq!(set.impls[0].target_type, "Foo");
        assert_eq!(set.impls[0].trait_name.as_deref(), Some("Greet"));
    }

    #[test]
    fn cids_are_deterministic_and_self_identifying() {
        let f = parse_file(
            r#"
            struct P { x: u32 }
            enum E { A, B(u32) }
            trait T { fn f(&self) -> u32; }
            impl T for P { fn f(&self) -> u32 { self.x } }
        "#,
        );
        let set_a = lift_file_type_decls(&f, Some("test.rs"));
        let set_b = lift_file_type_decls(&f, Some("test.rs"));
        for (a, b) in set_a.structs.iter().zip(set_b.structs.iter()) {
            assert_eq!(a.cid, b.cid);
            assert!(a.cid.starts_with("blake3-512:"));
            assert_eq!(a.cid.len(), 139);
        }
        for (a, b) in set_a.enums.iter().zip(set_b.enums.iter()) {
            assert_eq!(a.cid, b.cid);
        }
        for (a, b) in set_a.traits.iter().zip(set_b.traits.iter()) {
            assert_eq!(a.cid, b.cid);
        }
        for (a, b) in set_a.impls.iter().zip(set_b.impls.iter()) {
            assert_eq!(a.cid, b.cid);
        }
    }
}
