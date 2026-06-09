// SPDX-License-Identifier: Apache-2.0
//
// sugar-lift-rust-tests
//
// Rust parity for sugar-lift-py-tests' assertion-consistency path:
// recognize scalar assertions inside #[test] functions and emit inv-only
// ContractDecls. The verifier's existing consistency pass checks those closed
// invariants with raw SAT: SAT => consistent/discharged; UNSAT => refused.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;

use quote::ToTokens;
use sugar_ir_symbolic::{
    and_, atomic_, eq, gt, gte, lt, lte, make_var, ne, not_, num, or_, real_const, str_const,
    ConstValue, ContractDecl, Formula, Term,
};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{BinOp, Expr, ExprLit, Item, Lit, Stmt, Token, UnOp};

#[derive(Debug, Clone)]
pub struct LiftWarning {
    pub source_path: String,
    pub item_name: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct AdapterOutput {
    pub decls: Vec<ContractDecl>,
    pub warnings: Vec<LiftWarning>,
    pub seen: usize,
    pub lifted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LiftOptions {
    pub target_cfg: Option<TargetCfg>,
}

impl LiftOptions {
    pub fn for_target_cfg(target_cfg: TargetCfg) -> Self {
        Self {
            target_cfg: Some(target_cfg),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TargetCfg {
    facts: BTreeMap<String, BTreeSet<Option<String>>>,
}

impl TargetCfg {
    pub fn from_rustc_cfg_facts<I, S>(facts: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Self::default();
        for raw in facts {
            out.insert_rustc_cfg_fact(raw.as_ref())?;
        }
        Ok(out)
    }

    pub fn from_rustc_cfg_text(text: &str) -> Result<Self, String> {
        Self::from_rustc_cfg_facts(text.lines())
    }

    fn insert_rustc_cfg_fact(&mut self, raw: &str) -> Result<(), String> {
        let fact = raw.trim();
        if fact.is_empty() {
            return Ok(());
        }
        let (key, value) = if let Some(eq) = fact.find('=') {
            let key = fact[..eq].trim();
            let value = parse_rustc_cfg_quoted_value(fact[eq + 1..].trim())?;
            (key, Some(value))
        } else {
            (fact, None)
        };
        if key.is_empty() {
            return Err(format!("empty cfg key in `{fact}`"));
        }
        self.facts.entry(key.to_string()).or_default().insert(value);
        Ok(())
    }

    fn contains_name(&self, name: &str) -> bool {
        self.facts
            .get(name)
            .is_some_and(|values| values.contains(&None))
    }

    fn contains_key_value(&self, key: &str, value: &str) -> bool {
        self.facts
            .get(key)
            .is_some_and(|values| values.contains(&Some(value.to_string())))
    }
}

fn parse_rustc_cfg_quoted_value(raw: &str) -> Result<String, String> {
    let lit = syn::parse_str::<syn::LitStr>(raw)
        .map_err(|e| format!("cfg value must be a quoted Rust string `{raw}`: {e}"))?;
    Ok(lit.value())
}

pub fn lift_file(file: &syn::File, source_path: &str) -> AdapterOutput {
    lift_file_with_options(file, source_path, &LiftOptions::default())
}

pub fn lift_file_with_options(
    file: &syn::File,
    source_path: &str,
    options: &LiftOptions,
) -> AdapterOutput {
    let mut out = AdapterOutput::default();
    let mut modules = Vec::new();
    walk_items(&file.items, source_path, &mut modules, options, &mut out);
    out
}

fn walk_items(
    items: &[Item],
    source_path: &str,
    modules: &mut Vec<String>,
    options: &LiftOptions,
    out: &mut AdapterOutput,
) {
    for item in items {
        match item {
            Item::Fn(f) => visit_test_fn(f, source_path, modules, options, out),
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    let module_name = scoped_test_name(source_path, modules, &m.ident.to_string());
                    match cfg_eval_for_attrs(&m.attrs, options) {
                        CfgEval::Active => {}
                        CfgEval::Inactive(reason) => {
                            out.warnings.push(LiftWarning {
                                source_path: source_path.to_string(),
                                item_name: module_name,
                                reason: format!(
                                    "rust test assertions: inactive cfg; skipped module: {reason}"
                                ),
                            });
                            continue;
                        }
                        CfgEval::Ambiguous(reason) => {
                            out.warnings.push(LiftWarning {
                                source_path: source_path.to_string(),
                                item_name: module_name,
                                reason: format!(
                                    "rust test assertions: ambiguous cfg; skipped module: {reason}"
                                ),
                            });
                            continue;
                        }
                    }
                    modules.push(m.ident.to_string());
                    walk_items(items, source_path, modules, options, out);
                    modules.pop();
                }
            }
            _ => {}
        }
    }
}

fn visit_test_fn(
    f: &syn::ItemFn,
    source_path: &str,
    modules: &[String],
    options: &LiftOptions,
    out: &mut AdapterOutput,
) {
    if !has_test_attr(&f.attrs) {
        return;
    }

    let test_name = scoped_test_name(source_path, modules, &f.sig.ident.to_string());
    match cfg_eval_for_attrs(&f.attrs, options) {
        CfgEval::Active => {}
        CfgEval::Inactive(reason) => {
            out.warnings.push(LiftWarning {
                source_path: source_path.to_string(),
                item_name: test_name,
                reason: format!("rust test assertions: inactive cfg; skipped test: {reason}"),
            });
            return;
        }
        CfgEval::Ambiguous(reason) => {
            out.warnings.push(LiftWarning {
                source_path: source_path.to_string(),
                item_name: test_name,
                reason: format!("rust test assertions: ambiguous cfg; skipped test: {reason}"),
            });
            return;
        }
    }
    out.seen += 1;

    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    collect_assertion_entries(
        &f.block.stmts,
        &test_name,
        options,
        &mut entries,
        &mut skipped,
    );

    if !skipped.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.to_string(),
            item_name: test_name.clone(),
            reason: format!(
                "rust test assertions: unsupported assertion surface; released to layer 0: {}",
                skipped.join("; ")
            ),
        });
    }

    if entries.is_empty() {
        out.warnings.push(LiftWarning {
            source_path: source_path.to_string(),
            item_name: test_name,
            reason: "rust test assertions: no liftable scalar assertions".to_string(),
        });
        return;
    }

    for (name, atoms) in group_assertions(entries, &test_name) {
        out.decls.push(ContractDecl {
            name,
            pre: None,
            post: None,
            inv: Some(and_(atoms)),
            out_binding: "out".to_string(),
            evidence: None,
            panic_loci: Vec::new(),
            concept_hint: None,
        });
    }
    out.lifted += 1;
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
    })
}

fn scoped_test_name(source_path: &str, modules: &[String], fn_name: &str) -> String {
    if modules.is_empty() {
        format!("{source_path}::{fn_name}")
    } else {
        format!("{source_path}::{}::{fn_name}", modules.join("::"))
    }
}

struct AssertionEntry {
    name: Option<String>,
    atom: Rc<Formula>,
}

fn group_assertions(
    entries: Vec<AssertionEntry>,
    fallback_name: &str,
) -> Vec<(String, Vec<Rc<Formula>>)> {
    let mut groups: Vec<(String, Vec<Rc<Formula>>)> = Vec::new();
    for entry in entries {
        let name = entry.name.unwrap_or_else(|| fallback_name.to_string());
        if let Some((_, atoms)) = groups
            .iter_mut()
            .find(|(group_name, _)| group_name == &name)
        {
            atoms.push(entry.atom);
        } else {
            groups.push((name, vec![entry.atom]));
        }
    }
    groups
}

fn collect_assertion_entries(
    stmts: &[Stmt],
    local_scope: &str,
    options: &LiftOptions,
    entries: &mut Vec<AssertionEntry>,
    skipped: &mut Vec<String>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Macro(m) => match cfg_eval_for_attrs(&m.attrs, options) {
                CfgEval::Active => collect_macro(
                    &m.mac.path,
                    m.mac.tokens.clone(),
                    local_scope,
                    entries,
                    skipped,
                ),
                CfgEval::Inactive(reason) => {
                    skipped.push(format!("inactive cfg on assertion; skipped: {reason}"));
                }
                CfgEval::Ambiguous(reason) => {
                    skipped.push(format!("ambiguous cfg on assertion; skipped: {reason}"));
                }
            },
            Stmt::Expr(Expr::Macro(m), _) => match cfg_eval_for_attrs(&m.attrs, options) {
                CfgEval::Active => collect_macro(
                    &m.mac.path,
                    m.mac.tokens.clone(),
                    local_scope,
                    entries,
                    skipped,
                ),
                CfgEval::Inactive(reason) => {
                    skipped.push(format!("inactive cfg on assertion; skipped: {reason}"));
                }
                CfgEval::Ambiguous(reason) => {
                    skipped.push(format!("ambiguous cfg on assertion; skipped: {reason}"));
                }
            },
            _ => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CfgEval {
    Active,
    Inactive(String),
    Ambiguous(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CfgPredicate {
    Name(String),
    KeyValue(String, String),
    All(Vec<CfgPredicate>),
    Any(Vec<CfgPredicate>),
    Not(Box<CfgPredicate>),
}

impl Parse for CfgPredicate {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path: syn::Path = input.parse()?;
        let name = path_to_name(&path);
        if input.peek(Token![=]) {
            let _: Token![=] = input.parse()?;
            let value: syn::LitStr = input.parse()?;
            return Ok(CfgPredicate::KeyValue(name, value.value()));
        }
        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let args = Punctuated::<CfgPredicate, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect::<Vec<_>>();
            return match name.as_str() {
                "all" => Ok(CfgPredicate::All(args)),
                "any" => Ok(CfgPredicate::Any(args)),
                "not" if args.len() == 1 => Ok(CfgPredicate::Not(Box::new(
                    args.into_iter().next().unwrap(),
                ))),
                "not" => Err(syn::Error::new_spanned(
                    path,
                    "cfg not(...) expects exactly one predicate",
                )),
                _ => Err(syn::Error::new_spanned(
                    path,
                    "unsupported cfg predicate function",
                )),
            };
        }
        Ok(CfgPredicate::Name(name))
    }
}

impl fmt::Display for CfgPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CfgPredicate::Name(name) => f.write_str(name),
            CfgPredicate::KeyValue(key, value) => write!(f, "{key} = {value:?}"),
            CfgPredicate::All(predicates) => write!(
                f,
                "all({})",
                predicates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            CfgPredicate::Any(predicates) => write!(
                f,
                "any({})",
                predicates
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            CfgPredicate::Not(predicate) => write!(f, "not({predicate})"),
        }
    }
}

fn cfg_eval_for_attrs(attrs: &[syn::Attribute], options: &LiftOptions) -> CfgEval {
    let mut saw_cfg = false;
    for attr in attrs {
        if !attr.path().is_ident("cfg") {
            continue;
        }
        saw_cfg = true;
        let predicate = match attr.parse_args::<CfgPredicate>() {
            Ok(predicate) => predicate,
            Err(e) => {
                return CfgEval::Ambiguous(format!(
                    "cannot parse cfg `{}`: {e}",
                    attr.to_token_stream()
                ));
            }
        };
        match cfg_eval_predicate(&predicate, options.target_cfg.as_ref()) {
            CfgEval::Active => {}
            CfgEval::Inactive(reason) => return CfgEval::Inactive(reason),
            CfgEval::Ambiguous(reason) => return CfgEval::Ambiguous(reason),
        }
    }
    if saw_cfg {
        CfgEval::Active
    } else {
        CfgEval::Active
    }
}

fn cfg_eval_predicate(predicate: &CfgPredicate, target_cfg: Option<&TargetCfg>) -> CfgEval {
    match predicate {
        CfgPredicate::Name(name) => {
            if name == "test" {
                return CfgEval::Active;
            }
            let Some(target_cfg) = target_cfg else {
                return CfgEval::Ambiguous(format!(
                    "no explicit target cfg facts for `{predicate}`"
                ));
            };
            if target_cfg.contains_name(name) {
                CfgEval::Active
            } else {
                CfgEval::Inactive(predicate.to_string())
            }
        }
        CfgPredicate::KeyValue(key, value) => {
            let Some(target_cfg) = target_cfg else {
                return CfgEval::Ambiguous(format!(
                    "no explicit target cfg facts for `{predicate}`"
                ));
            };
            if target_cfg.contains_key_value(key, value) {
                CfgEval::Active
            } else {
                CfgEval::Inactive(predicate.to_string())
            }
        }
        CfgPredicate::All(predicates) => {
            let mut ambiguous = None;
            for child in predicates {
                match cfg_eval_predicate(child, target_cfg) {
                    CfgEval::Active => {}
                    CfgEval::Inactive(reason) => return CfgEval::Inactive(reason),
                    CfgEval::Ambiguous(reason) => {
                        ambiguous.get_or_insert(reason);
                    }
                }
            }
            if let Some(reason) = ambiguous {
                CfgEval::Ambiguous(reason)
            } else {
                CfgEval::Active
            }
        }
        CfgPredicate::Any(predicates) => {
            let mut inactive = Vec::new();
            let mut ambiguous = None;
            for child in predicates {
                match cfg_eval_predicate(child, target_cfg) {
                    CfgEval::Active => return CfgEval::Active,
                    CfgEval::Inactive(reason) => inactive.push(reason),
                    CfgEval::Ambiguous(reason) => {
                        ambiguous.get_or_insert(reason);
                    }
                }
            }
            if let Some(reason) = ambiguous {
                CfgEval::Ambiguous(reason)
            } else {
                CfgEval::Inactive(format!("any inactive: {}", inactive.join("; ")))
            }
        }
        CfgPredicate::Not(child) => match cfg_eval_predicate(child, target_cfg) {
            CfgEval::Active => CfgEval::Inactive(predicate.to_string()),
            CfgEval::Inactive(_) => CfgEval::Active,
            CfgEval::Ambiguous(reason) => CfgEval::Ambiguous(reason),
        },
    }
}

fn collect_macro(
    path: &syn::Path,
    tokens: proc_macro2::TokenStream,
    local_scope: &str,
    entries: &mut Vec<AssertionEntry>,
    skipped: &mut Vec<String>,
) {
    match assertions_from_macro(path, tokens, local_scope) {
        Ok(macro_entries) => entries.extend(macro_entries),
        Err(reason) => skipped.push(reason),
    }
}

fn assertions_from_macro(
    path: &syn::Path,
    tokens: proc_macro2::TokenStream,
    local_scope: &str,
) -> Result<Vec<AssertionEntry>, String> {
    let Some(name) = path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    else {
        return Ok(Vec::new());
    };
    match name.as_str() {
        "assert_eq" => {
            let args = parse_macro_args(tokens).map_err(|e| format!("assert_eq!: {e}"))?;
            if args.exprs.len() < 2 {
                return Err("assert_eq!: expected at least 2 arguments".to_string());
            }
            let lhs = translate_assertion_term(&args.exprs[0], local_scope)
                .map_err(|e| format!("assert_eq!: {e}"))?;
            let rhs = translate_assertion_term(&args.exprs[1], local_scope)
                .map_err(|e| format!("assert_eq!: {e}"))?;
            Ok(vec![assertion_entry_from_eq(lhs, rhs, local_scope)])
        }
        "assert" => {
            let args = parse_macro_args(tokens).map_err(|e| format!("assert!: {e}"))?;
            let Some(first) = args.exprs.first() else {
                return Err("assert!: expected a condition".to_string());
            };
            let entry = translate_bool_assertion(first, local_scope)
                .map_err(|e| format!("assert!: {e}"))?;
            Ok(vec![entry])
        }
        "assert_all" | "assert_none" => {
            let args = parse_macro_args(tokens).map_err(|e| format!("{name}!: {e}"))?;
            assertion_entries_from_ascii_macro(name.as_str(), &args.exprs)
        }
        other if other.starts_with("assert") || other.starts_with("debug_assert") => {
            Err(format!("{other}!: unsupported assertion macro"))
        }
        _ => Ok(Vec::new()),
    }
}

fn assertion_entries_from_ascii_macro(
    macro_name: &str,
    exprs: &[Expr],
) -> Result<Vec<AssertionEntry>, String> {
    if exprs.len() < 2 {
        return Err(format!(
            "{macro_name}!: expected predicate name and at least one literal source"
        ));
    }
    let predicate = ascii_macro_predicate_name(&exprs[0]).ok_or_else(|| {
        format!(
            "{macro_name}!: expected a simple ASCII predicate name, got `{}`",
            token_key(&exprs[0])
        )
    })?;
    let negate = macro_name == "assert_none";
    let mut entries = Vec::new();
    for source in &exprs[1..] {
        let value = literal_string_value(source).ok_or_else(|| {
            format!(
                "{macro_name}!: expected string literal source, got `{}`",
                token_key(source)
            )
        })?;
        for ch in value.chars() {
            let atom = ascii_char_class_atom(&predicate, str_const(ch.to_string()))
                .ok_or_else(|| unsupported_ascii_macro_predicate(&predicate))?;
            entries.push(AssertionEntry {
                name: None,
                atom: if negate { not_(atom) } else { atom },
            });
        }
        for byte in value.as_bytes() {
            let atom = ascii_byte_class_atom(&predicate, num(i64::from(*byte)))
                .ok_or_else(|| unsupported_ascii_macro_predicate(&predicate))?;
            entries.push(AssertionEntry {
                name: None,
                atom: if negate { not_(atom) } else { atom },
            });
        }
    }
    Ok(entries)
}

fn ascii_macro_predicate_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) => path.path.get_ident().map(|ident| ident.to_string()),
        Expr::Paren(paren) => ascii_macro_predicate_name(&paren.expr),
        Expr::Group(group) => ascii_macro_predicate_name(&group.expr),
        _ => None,
    }
}

fn unsupported_ascii_macro_predicate(predicate: &str) -> String {
    if predicate == "is_alphabetic" {
        "unicode char predicate is_alphabetic is not lifted; z3 string theory has no Rust Unicode Alphabetic database"
            .to_string()
    } else {
        format!("unsupported bounded ASCII macro predicate `{predicate}`")
    }
}

struct MacroArgs {
    exprs: Vec<Expr>,
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let exprs = Punctuated::<Expr, Token![,]>::parse_terminated(input)?
            .into_iter()
            .collect();
        Ok(Self { exprs })
    }
}

fn parse_macro_args(tokens: proc_macro2::TokenStream) -> syn::Result<MacroArgs> {
    syn::parse2(tokens)
}

fn translate_bool_assertion(expr: &Expr, local_scope: &str) -> Result<AssertionEntry, String> {
    if let Some(entry) = translate_string_predicate_assertion(expr, local_scope)? {
        return Ok(entry);
    }
    if let Some(entry) = translate_literal_iterator_assertion(expr, local_scope)? {
        return Ok(entry);
    }
    match expr {
        Expr::Binary(binary) => translate_binary_bool_assertion(binary, local_scope),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Not(_)) => {
            if let Some(entry) = translate_string_predicate_assertion(&unary.expr, local_scope)? {
                return Ok(AssertionEntry {
                    name: entry.name,
                    atom: not_(entry.atom),
                });
            }
            if let Ok(term) = translate_term(&unary.expr) {
                Ok(assertion_entry_from_eq(
                    term,
                    bool_const(false),
                    local_scope,
                ))
            } else {
                let entry = translate_bool_assertion(&unary.expr, local_scope)?;
                Ok(AssertionEntry {
                    name: entry.name,
                    atom: not_(entry.atom),
                })
            }
        }
        Expr::Call(_) | Expr::MethodCall(_) | Expr::Await(_) | Expr::Field(_) => {
            let term = translate_term(expr)?;
            if is_refinement_predicate_term(term.as_ref()) {
                return Err(format!(
                    "refinement predicate remains out of this exact-value slice `{}`",
                    token_key(expr)
                ));
            }
            Ok(assertion_entry_from_eq(term, bool_const(true), local_scope))
        }
        Expr::Paren(paren) => translate_bool_assertion(&paren.expr, local_scope),
        Expr::Group(group) => translate_bool_assertion(&group.expr, local_scope),
        other => Err(format!(
            "only scalar equality is liftable, got `{}`",
            token_key(other)
        )),
    }
}

fn translate_binary_bool_assertion(
    binary: &syn::ExprBinary,
    local_scope: &str,
) -> Result<AssertionEntry, String> {
    match &binary.op {
        BinOp::And(_) | BinOp::Or(_) => {
            let left = translate_bool_assertion(&binary.left, local_scope)?;
            let right = translate_bool_assertion(&binary.right, local_scope)?;
            let name = common_assertion_name(&left.name, &right.name);
            let atom = if matches!(binary.op, BinOp::And(_)) {
                and_(vec![left.atom, right.atom])
            } else {
                or_(vec![left.atom, right.atom])
            };
            Ok(AssertionEntry { name, atom })
        }
        BinOp::Eq(_) | BinOp::Ne(_) | BinOp::Lt(_) | BinOp::Le(_) | BinOp::Gt(_) | BinOp::Ge(_) => {
            let op = relation_from_binop(&binary.op)
                .expect("comparison op matched but did not map to relation");
            let lhs = translate_assertion_term(&binary.left, local_scope)?;
            let rhs = translate_assertion_term(&binary.right, local_scope)?;
            Ok(assertion_entry_from_relation(lhs, rhs, op, local_scope))
        }
        _ => Err(format!(
            "only scalar comparison/connective assertions are liftable, got `{}`",
            token_key(binary)
        )),
    }
}

fn common_assertion_name(left: &Option<String>, right: &Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) if left == right => Some(left.clone()),
        _ => None,
    }
}

fn assertion_entry_from_eq(lhs: Rc<Term>, rhs: Rc<Term>, local_scope: &str) -> AssertionEntry {
    assertion_entry_from_relation(lhs, rhs, RelationOp::Eq, local_scope)
}

#[derive(Clone, Copy)]
enum RelationOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl RelationOp {
    fn operator_call_name(self) -> &'static str {
        match self {
            RelationOp::Eq | RelationOp::Ne => "eq",
            RelationOp::Lt => "lt",
            RelationOp::Le => "le",
            RelationOp::Gt => "gt",
            RelationOp::Ge => "ge",
        }
    }

    fn operator_asserted_result(self) -> bool {
        !matches!(self, RelationOp::Ne)
    }
}

fn relation_from_binop(op: &BinOp) -> Option<RelationOp> {
    match op {
        BinOp::Eq(_) => Some(RelationOp::Eq),
        BinOp::Ne(_) => Some(RelationOp::Ne),
        BinOp::Lt(_) => Some(RelationOp::Lt),
        BinOp::Le(_) => Some(RelationOp::Le),
        BinOp::Gt(_) => Some(RelationOp::Gt),
        BinOp::Ge(_) => Some(RelationOp::Ge),
        _ => None,
    }
}

fn translate_string_predicate_assertion(
    expr: &Expr,
    local_scope: &str,
) -> Result<Option<AssertionEntry>, String> {
    match expr {
        Expr::Paren(paren) => translate_string_predicate_assertion(&paren.expr, local_scope),
        Expr::Group(group) => translate_string_predicate_assertion(&group.expr, local_scope),
        Expr::MethodCall(call) => {
            let method = call.method.to_string();
            match method.as_str() {
                "contains" => {
                    let Some(receiver) = string_or_char_literal_term(&call.receiver) else {
                        return Ok(None);
                    };
                    if call.args.len() != 1 {
                        return Err("string contains predicate expects one literal pattern".to_string());
                    }
                    let Some(pattern) = string_or_char_literal_term(&call.args[0]) else {
                        return Err(format!(
                            "string contains predicate needs a string/char literal pattern, got `{}`",
                            token_key(&call.args[0])
                        ));
                    };
                    let name = method_call_assertion_name(
                        "contains",
                        vec![receiver.clone(), pattern.clone()],
                        local_scope,
                    );
                    Ok(Some(AssertionEntry {
                        name,
                        atom: atomic_("contains", vec![receiver, pattern]),
                    }))
                }
                "starts_with" | "ends_with" => {
                    let Some(receiver) = string_or_char_literal_term(&call.receiver) else {
                        return Ok(None);
                    };
                    if call.args.len() != 1 {
                        return Err(format!("{method} predicate expects one literal pattern"));
                    }
                    let Some(pattern) = string_or_char_literal_term(&call.args[0]) else {
                        return Err(format!(
                            "{method} predicate needs a string/char literal pattern, got `{}`",
                            token_key(&call.args[0])
                        ));
                    };
                    let name = method_call_assertion_name(
                        method.as_str(),
                        vec![receiver.clone(), pattern.clone()],
                        local_scope,
                    );
                    let atom_name = if method == "starts_with" {
                        "prefix-of"
                    } else {
                        "suffix-of"
                    };
                    Ok(Some(AssertionEntry {
                        name,
                        atom: atomic_(atom_name, vec![pattern, receiver]),
                    }))
                }
                "is_ascii" => {
                    if !call.args.is_empty() {
                        return Err("is_ascii predicate expects no arguments".to_string());
                    }
                    if let Some(receiver) = string_or_char_literal_term(&call.receiver) {
                        let name = method_call_assertion_name(
                            "is_ascii",
                            vec![receiver.clone()],
                            local_scope,
                        );
                        return Ok(Some(AssertionEntry {
                            name,
                            atom: atomic_("str.is_ascii", vec![receiver]),
                        }));
                    }
                    let Some(bytes) = literal_byte_string_value(&call.receiver) else {
                        return Ok(None);
                    };
                    let atoms = bytes
                        .into_iter()
                        .map(|b| byte_is_ascii_formula(num(i64::from(b))))
                        .collect::<Vec<_>>();
                    let atom = if atoms.is_empty() {
                        eq(bool_const(true), bool_const(true))
                    } else {
                        and_(atoms)
                    };
                    Ok(Some(AssertionEntry { name: None, atom }))
                }
                "is_ascii_alphabetic" => {
                    let Some(receiver) = char_literal_term(&call.receiver) else {
                        return Ok(None);
                    };
                    if !call.args.is_empty() {
                        return Err("is_ascii_alphabetic predicate expects no arguments".to_string());
                    }
                    let name = method_call_assertion_name(
                        "is_ascii_alphabetic",
                        vec![receiver.clone()],
                        local_scope,
                    );
                    Ok(Some(AssertionEntry {
                        name,
                        atom: atomic_("str.is_ascii_alphabetic", vec![receiver]),
                    }))
                }
                "is_ascii_digit" => ascii_char_class_assertion(call, local_scope, "str.is_ascii_digit"),
                "is_ascii_alphanumeric" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_alphanumeric",
                ),
                "is_ascii_octdigit" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_octdigit",
                ),
                "is_ascii_lowercase" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_lowercase",
                ),
                "is_ascii_uppercase" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_uppercase",
                ),
                "is_ascii_hexdigit" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_hexdigit",
                ),
                "is_ascii_punctuation" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_punctuation",
                ),
                "is_ascii_graphic" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_graphic",
                ),
                "is_ascii_whitespace" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_whitespace",
                ),
                "is_ascii_control" => ascii_char_class_assertion(
                    call,
                    local_scope,
                    "str.is_ascii_control",
                ),
                "is_alphabetic" if char_literal_term(&call.receiver).is_some() => Err(
                    "unicode char predicate is_alphabetic is not lifted; z3 string theory has no Rust Unicode Alphabetic database"
                        .to_string(),
                ),
                _ => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

fn ascii_char_class_assertion(
    call: &syn::ExprMethodCall,
    local_scope: &str,
    atom_name: &str,
) -> Result<Option<AssertionEntry>, String> {
    let Some(receiver) = char_literal_term(&call.receiver) else {
        return Ok(None);
    };
    if !call.args.is_empty() {
        return Err(format!("{} predicate expects no arguments", call.method));
    }
    let method = call.method.to_string();
    let name = method_call_assertion_name(method.as_str(), vec![receiver.clone()], local_scope);
    Ok(Some(AssertionEntry {
        name,
        atom: atomic_(atom_name, vec![receiver]),
    }))
}

fn translate_literal_iterator_assertion(
    expr: &Expr,
    _local_scope: &str,
) -> Result<Option<AssertionEntry>, String> {
    let Expr::MethodCall(call) = expr else {
        return Ok(None);
    };
    let method = call.method.to_string();
    if !matches!(method.as_str(), "all" | "any") {
        return Ok(None);
    }
    if call.args.len() != 1 {
        return Err(format!("{method} predicate expects one closure"));
    }
    let Some(closure) = call.args.first().and_then(|expr| match expr {
        Expr::Closure(closure) => Some(closure),
        _ => None,
    }) else {
        return Ok(None);
    };
    if closure.inputs.len() != 1 {
        return Err(format!("{method} predicate expects one closure parameter"));
    }
    let param_name = closure
        .inputs
        .first()
        .and_then(|pat| match pat {
            syn::Pat::Ident(ident) => Some(ident.ident.to_string()),
            _ => None,
        })
        .ok_or_else(|| format!("{method} predicate requires a simple identifier parameter"))?;

    let Some((iter_kind, elements)) = literal_iterator_elements(&call.receiver)? else {
        return Ok(None);
    };
    let mut atoms = Vec::new();
    for element in elements {
        atoms.push(iterator_element_predicate_atom(
            closure.body.as_ref(),
            &param_name,
            element,
            iter_kind,
        )?);
    }
    let atom = if method == "all" {
        if atoms.is_empty() {
            eq(bool_const(true), bool_const(true))
        } else {
            and_(atoms)
        }
    } else if atoms.is_empty() {
        eq(bool_const(true), bool_const(false))
    } else {
        or_(atoms)
    };
    Ok(Some(AssertionEntry { name: None, atom }))
}

#[derive(Clone, Copy)]
enum IteratorKind {
    Chars,
    Bytes,
}

fn iterator_element_predicate_atom(
    body: &Expr,
    param_name: &str,
    element: Rc<Term>,
    iter_kind: IteratorKind,
) -> Result<Rc<Formula>, String> {
    let Expr::MethodCall(call) = body else {
        return Err(format!(
            "iterator closure body must be a simple method call, got `{}`",
            token_key(body)
        ));
    };
    if !call.args.is_empty() {
        return Err(format!(
            "iterator closure predicate `{}` expects no arguments",
            call.method
        ));
    }
    if !matches_param_receiver(&call.receiver, param_name) {
        return Err(format!(
            "iterator closure predicate must read its bound parameter `{param_name}`"
        ));
    }
    let method = call.method.to_string();
    match iter_kind {
        IteratorKind::Chars => ascii_char_class_atom(&method, element).ok_or_else(|| {
            if method == "is_alphabetic" {
                "unicode char predicate is_alphabetic is not lifted; z3 string theory has no Rust Unicode Alphabetic database"
                    .to_string()
            } else {
                format!("unsupported char iterator predicate `{method}`")
            }
        }),
        IteratorKind::Bytes => ascii_byte_class_atom(&method, element)
            .ok_or_else(|| format!("unsupported byte iterator predicate `{method}`")),
    }
}

fn ascii_char_class_atom(method: &str, receiver: Rc<Term>) -> Option<Rc<Formula>> {
    let atom_name = match method {
        "is_ascii" => "str.is_ascii",
        "is_ascii_alphabetic" => "str.is_ascii_alphabetic",
        "is_ascii_alphanumeric" => "str.is_ascii_alphanumeric",
        "is_ascii_digit" => "str.is_ascii_digit",
        "is_ascii_octdigit" => "str.is_ascii_octdigit",
        "is_ascii_lowercase" => "str.is_ascii_lowercase",
        "is_ascii_uppercase" => "str.is_ascii_uppercase",
        "is_ascii_hexdigit" => "str.is_ascii_hexdigit",
        "is_ascii_punctuation" => "str.is_ascii_punctuation",
        "is_ascii_graphic" => "str.is_ascii_graphic",
        "is_ascii_whitespace" => "str.is_ascii_whitespace",
        "is_ascii_control" => "str.is_ascii_control",
        _ => return None,
    };
    Some(atomic_(atom_name, vec![receiver]))
}

fn ascii_byte_class_atom(method: &str, byte: Rc<Term>) -> Option<Rc<Formula>> {
    match method {
        "is_ascii" => Some(byte_is_ascii_formula(byte)),
        "is_ascii_alphabetic" => Some(or_(vec![
            byte_range(byte.clone(), b'A', b'Z'),
            byte_range(byte, b'a', b'z'),
        ])),
        "is_ascii_alphanumeric" => Some(or_(vec![
            byte_range(byte.clone(), b'A', b'Z'),
            byte_range(byte.clone(), b'a', b'z'),
            byte_range(byte, b'0', b'9'),
        ])),
        "is_ascii_digit" => Some(byte_range(byte, b'0', b'9')),
        "is_ascii_octdigit" => Some(byte_range(byte, b'0', b'7')),
        "is_ascii_lowercase" => Some(byte_range(byte, b'a', b'z')),
        "is_ascii_uppercase" => Some(byte_range(byte, b'A', b'Z')),
        "is_ascii_hexdigit" => Some(or_(vec![
            byte_range(byte.clone(), b'0', b'9'),
            byte_range(byte.clone(), b'A', b'F'),
            byte_range(byte, b'a', b'f'),
        ])),
        "is_ascii_punctuation" => Some(or_(vec![
            byte_range(byte.clone(), b'!', b'/'),
            byte_range(byte.clone(), b':', b'@'),
            byte_range(byte.clone(), b'[', b'`'),
            byte_range(byte, b'{', b'~'),
        ])),
        "is_ascii_graphic" => Some(byte_range(byte, b'!', b'~')),
        "is_ascii_whitespace" => Some(or_(vec![
            eq(byte.clone(), num(i64::from(b' '))),
            eq(byte.clone(), num(9)),
            eq(byte.clone(), num(10)),
            eq(byte.clone(), num(12)),
            eq(byte, num(13)),
        ])),
        "is_ascii_control" => Some(or_(vec![
            byte_range(byte.clone(), 0u8, 31u8),
            eq(byte, num(127)),
        ])),
        _ => None,
    }
}

fn byte_is_ascii_formula(byte: Rc<Term>) -> Rc<Formula> {
    and_(vec![gte(byte.clone(), num(0)), lte(byte, num(127))])
}

fn byte_range(byte: Rc<Term>, low: u8, high: u8) -> Rc<Formula> {
    and_(vec![
        gte(byte.clone(), num(i64::from(low))),
        lte(byte, num(i64::from(high))),
    ])
}

fn literal_iterator_elements(expr: &Expr) -> Result<Option<(IteratorKind, Vec<Rc<Term>>)>, String> {
    match expr {
        Expr::MethodCall(call) if call.args.is_empty() && call.method == "chars" => {
            let Some(value) = literal_string_value(&call.receiver) else {
                return Ok(None);
            };
            let elements = value
                .chars()
                .map(|ch| str_const(ch.to_string()))
                .collect::<Vec<_>>();
            Ok(Some((IteratorKind::Chars, elements)))
        }
        Expr::MethodCall(call) if call.args.is_empty() && call.method == "iter" => {
            let Some(bytes) = literal_byte_string_value(&call.receiver) else {
                return Ok(None);
            };
            let elements = bytes.into_iter().map(|b| num(i64::from(b))).collect();
            Ok(Some((IteratorKind::Bytes, elements)))
        }
        Expr::Paren(paren) => literal_iterator_elements(&paren.expr),
        Expr::Group(group) => literal_iterator_elements(&group.expr),
        _ => Ok(None),
    }
}

fn literal_string_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(s.value()),
        Expr::Paren(paren) => literal_string_value(&paren.expr),
        Expr::Group(group) => literal_string_value(&group.expr),
        _ => None,
    }
}

fn literal_byte_string_value(expr: &Expr) -> Option<Vec<u8>> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::ByteStr(bytes),
            ..
        }) => Some(bytes.value()),
        Expr::Paren(paren) => literal_byte_string_value(&paren.expr),
        Expr::Group(group) => literal_byte_string_value(&group.expr),
        _ => None,
    }
}

fn matches_param_receiver(expr: &Expr, param_name: &str) -> bool {
    match expr {
        Expr::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == param_name),
        Expr::Paren(paren) => matches_param_receiver(&paren.expr, param_name),
        Expr::Group(group) => matches_param_receiver(&group.expr, param_name),
        _ => false,
    }
}

fn method_call_assertion_name(
    method: &str,
    args: Vec<Rc<Term>>,
    local_scope: &str,
) -> Option<String> {
    let term = Term::Ctor {
        name: format!("method:{method}"),
        args,
    };
    callsite_assertion_name(&term, local_scope)
}

fn assertion_entry_from_relation(
    lhs: Rc<Term>,
    rhs: Rc<Term>,
    op: RelationOp,
    local_scope: &str,
) -> AssertionEntry {
    if let Some(tag) =
        constructor_operator_tag(lhs.as_ref()).or_else(|| constructor_operator_tag(rhs.as_ref()))
    {
        return AssertionEntry {
            name: None,
            atom: constructor_operator_atom(lhs, rhs, op, &tag),
        };
    }

    let name = if is_ground_value(lhs.as_ref()) {
        callsite_assertion_name(rhs.as_ref(), local_scope)
    } else if is_ground_value(rhs.as_ref()) {
        callsite_assertion_name(lhs.as_ref(), local_scope)
    } else {
        None
    };
    let atom = match op {
        RelationOp::Eq => eq(lhs, rhs),
        RelationOp::Ne => ne(lhs, rhs),
        RelationOp::Lt => lt(lhs, rhs),
        RelationOp::Le => lte(lhs, rhs),
        RelationOp::Gt => gt(lhs, rhs),
        RelationOp::Ge => gte(lhs, rhs),
    };
    AssertionEntry { name, atom }
}

fn constructor_operator_atom(
    lhs: Rc<Term>,
    rhs: Rc<Term>,
    op: RelationOp,
    tag: &str,
) -> Rc<Formula> {
    // Federated operator-dispatch shape: user-type operators are method calls,
    // so ==/!= lift as equality over the canonical eq call result, and
    // order operators lift as their own canonical call results. Java .equals
    // and Python __eq__ must mirror this byte-for-byte for the same TypeKey.
    let operator_call = Rc::new(Term::Ctor {
        name: format!("call:{}:{tag}", op.operator_call_name()),
        args: vec![lhs, rhs],
    });
    eq(operator_call, bool_const(op.operator_asserted_result()))
}

fn constructor_operator_tag(term: &Term) -> Option<String> {
    let Term::Ctor { name, .. } = term else {
        return None;
    };
    let callee = name.strip_prefix("call:")?;
    let final_segment = callee.rsplit("::").next().unwrap_or(callee);
    final_segment
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        .then(|| callee.to_string())
}

fn is_ground_value(term: &Term) -> bool {
    match term {
        Term::Const { .. } => true,
        Term::Var { name } if name.starts_with("literal:") => true,
        Term::Ctor { name, args } if is_ground_value_ctor(name) => {
            args.iter().all(|arg| is_ground_value(arg))
        }
        _ => false,
    }
}

fn is_ground_value_ctor(name: &str) -> bool {
    matches!(
        name,
        "+" | "-"
            | "*"
            | "int-div"
            | "int-rem"
            | "bit-and"
            | "bit-or"
            | "bit-xor"
            | "shift-left"
            | "shift-right"
            | "bit-not"
            | "ref"
            | "range"
            | "range_incl"
    )
}

fn bool_const(value: bool) -> Rc<Term> {
    Rc::new(Term::Const {
        value: ConstValue::Bool(value),
        sort: sugar_ir_symbolic::Sort::bool(),
    })
}

fn callsite_assertion_name(term: &Term, local_scope: &str) -> Option<String> {
    let Term::Ctor { name, .. } = term else {
        return None;
    };
    let callee = callsite_callee_name(name)?;
    Some(format!(
        "{callee}#euf#{}::assertion",
        canonical_callsite_sig(term, local_scope)
    ))
}

fn canonical_callsite_sig(term: &Term, local_scope: &str) -> String {
    let Term::Ctor { name, args } = term else {
        return term_key(term);
    };
    let Some(callee) = callsite_callee_name(name) else {
        return term_key(term);
    };
    let head = call_result_head(callee, args.len());
    let inner = args
        .iter()
        .map(|arg| {
            if callee.starts_with("method:") {
                canonical_method_arg_sig(arg, local_scope)
            } else {
                canonical_term_sig(arg)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("c:{head}({inner})")
}

fn callsite_callee_name(name: &str) -> Option<&str> {
    if name == "str.len" {
        return Some("method:len");
    }
    name.strip_prefix("call:")
        .or_else(|| name.starts_with("method:").then_some(name))
}

fn call_result_head(callee: &str, arity: usize) -> String {
    let safe = callee
        .chars()
        .map(|ch| {
            if ch.is_ascii() && ch.is_ascii_alphanumeric() {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("callresult_{safe}_a{arity}")
}

fn canonical_term_sig(term: &Term) -> String {
    match term {
        Term::Var { name } => format!("v:{name}"),
        Term::Const { value, .. } => match value {
            ConstValue::Int(value) => format!("i:{value}"),
            ConstValue::Real(value) => format!("r:{value}"),
            ConstValue::String(value) => format!("s:{value:?}"),
            ConstValue::Bool(value) => format!("b:{value}"),
        },
        Term::Ctor { name, args } => {
            let inner = args
                .iter()
                .map(|arg| canonical_term_sig(arg))
                .collect::<Vec<_>>()
                .join(",");
            format!("c:{name}({inner})")
        }
        _ => term_key(term),
    }
}

fn canonical_method_arg_sig(term: &Term, local_scope: &str) -> String {
    match term {
        Term::Var { name } if name.starts_with("literal:") => format!("v:{name}"),
        Term::Var { name } if is_unqualified_local_name(name) => {
            format!("v:{local_scope}::{name}")
        }
        Term::Var { name } => format!("v:{name}"),
        Term::Const { value, .. } => match value {
            ConstValue::Int(value) => format!("i:{value}"),
            ConstValue::Real(value) => format!("r:{value}"),
            ConstValue::String(value) => format!("s:{value:?}"),
            ConstValue::Bool(value) => format!("b:{value}"),
        },
        Term::Ctor { name, args } => {
            let inner = args
                .iter()
                .map(|arg| canonical_method_arg_sig(arg, local_scope))
                .collect::<Vec<_>>()
                .join(",");
            format!("c:{name}({inner})")
        }
        _ => term_key(term),
    }
}

fn is_unqualified_local_name(name: &str) -> bool {
    !name.contains("::")
}

fn is_refinement_predicate_term(term: &Term) -> bool {
    matches!(
        term,
        Term::Ctor { name, .. }
            if matches!(
                name.as_str(),
                "method:is_nan"
                    | "method:is_finite"
                    | "method:is_infinite"
                    | "method:is_normal"
                    | "method:is_subnormal"
                    | "method:is_sign_positive"
                    | "method:is_sign_negative"
            )
    )
}

fn term_key(term: &Term) -> String {
    format!("{term:?}")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn translate_term(expr: &Expr) -> Result<Rc<Term>, String> {
    match expr {
        Expr::Lit(lit) => translate_lit(lit),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => {
            let value = const_int(&unary.expr)
                .ok_or_else(|| format!("unsupported negative literal `{}`", token_key(expr)))?;
            Ok(num(-value))
        }
        Expr::Unary(unary) if matches!(unary.op, UnOp::Not(_)) => Ok(Rc::new(Term::Ctor {
            name: "bit-not".to_string(),
            args: vec![translate_term(&unary.expr)?],
        })),
        Expr::Path(path) => Ok(make_var(path_to_name(&path.path))),
        Expr::Call(call) => {
            if let Some(term) = type_id_of_call_term(&call.func, call.args.len())? {
                return Ok(term);
            }
            let mut args = Vec::new();
            for arg in &call.args {
                args.push(translate_term(arg)?);
            }
            Ok(Rc::new(Term::Ctor {
                name: format!("call:{}", expr_head_key(&call.func)),
                args,
            }))
        }
        Expr::Array(array) => literal_aggregate_term("Array", array.elems.iter(), expr),
        Expr::Tuple(tuple) => literal_aggregate_term("Tuple", tuple.elems.iter(), expr),
        Expr::MethodCall(call) => {
            if call.method == "len" && call.args.is_empty() {
                if let Some(receiver) = string_or_char_literal_term(&call.receiver) {
                    return Ok(Rc::new(Term::Ctor {
                        name: "str.len".to_string(),
                        args: vec![receiver],
                    }));
                }
            }
            let mut args = vec![translate_term(&call.receiver)?];
            for arg in &call.args {
                args.push(translate_term(arg)?);
            }
            let method = match &call.turbofish {
                Some(args) => format!("{}{}", call.method, angle_args_key(args)),
                None => call.method.to_string(),
            };
            Ok(Rc::new(Term::Ctor {
                name: format!("method:{method}"),
                args,
            }))
        }
        Expr::Await(await_expr) => Ok(Rc::new(Term::Ctor {
            name: "await".to_string(),
            args: vec![translate_term(&await_expr.base)?],
        })),
        Expr::Reference(reference) if reference.mutability.is_none() => Ok(Rc::new(Term::Ctor {
            name: "ref".to_string(),
            args: vec![translate_term(&reference.expr)?],
        })),
        Expr::Cast(cast) if is_shared_dyn_any_type(&cast.ty) => Ok(Rc::new(Term::Ctor {
            name: format!("cast:{}", type_key(&cast.ty)),
            args: vec![translate_term(&cast.expr)?],
        })),
        Expr::Range(range) => {
            let start = match &range.start {
                Some(expr) => translate_term(expr)?,
                None => make_var("_"),
            };
            let end = match &range.end {
                Some(expr) => translate_term(expr)?,
                None => make_var("_"),
            };
            let name = match range.limits {
                syn::RangeLimits::HalfOpen(_) => "range",
                syn::RangeLimits::Closed(_) => "range_incl",
            };
            Ok(Rc::new(Term::Ctor {
                name: name.to_string(),
                args: vec![start, end],
            }))
        }
        Expr::Field(field) => Ok(Rc::new(Term::Ctor {
            name: format!("field:{}", token_key(&field.member)),
            args: vec![translate_term(&field.base)?],
        })),
        Expr::Binary(binary) => {
            let Some(op) = term_binop_name(&binary.op) else {
                return Err(format!("unsupported term operator `{}`", token_key(expr)));
            };
            Ok(Rc::new(Term::Ctor {
                name: op.to_string(),
                args: vec![
                    translate_term(&binary.left)?,
                    translate_term(&binary.right)?,
                ],
            }))
        }
        Expr::Paren(paren) => translate_term(&paren.expr),
        Expr::Group(group) => translate_term(&group.expr),
        other => Err(format!("unsupported term `{}`", token_key(other))),
    }
}

fn translate_assertion_term(expr: &Expr, local_scope: &str) -> Result<Rc<Term>, String> {
    match expr {
        Expr::Const(const_block) => {
            let term = translate_expression_only_block(&const_block.block, "const")?;
            Ok(scope_const_block_locals(term, local_scope))
        }
        Expr::Path(path) if path.path.is_ident("None") => Ok(Rc::new(Term::Ctor {
            name: "call:None".to_string(),
            args: Vec::new(),
        })),
        Expr::Paren(paren) => translate_assertion_term(&paren.expr, local_scope),
        Expr::Group(group) => translate_assertion_term(&group.expr, local_scope),
        _ => translate_term(expr),
    }
}

fn scope_const_block_locals(term: Rc<Term>, local_scope: &str) -> Rc<Term> {
    match term.as_ref() {
        Term::Var { name } if should_scope_const_block_var(name) => {
            make_var(format!("{local_scope}::{name}"))
        }
        Term::Ctor { name, args } => Rc::new(Term::Ctor {
            name: name.clone(),
            args: args
                .iter()
                .map(|arg| scope_const_block_locals(arg.clone(), local_scope))
                .collect(),
        }),
        _ => term,
    }
}

fn should_scope_const_block_var(name: &str) -> bool {
    is_unqualified_local_name(name) && name != "_" && !name.starts_with("literal:")
}

fn translate_expression_only_block(block: &syn::Block, label: &str) -> Result<Rc<Term>, String> {
    match block.stmts.as_slice() {
        [Stmt::Expr(expr, None)] => translate_term(expr),
        _ => Err(format!(
            "{label} block is not an expression-only term `{}`",
            token_key(block)
        )),
    }
}

fn literal_aggregate_term<'a>(
    kind: &str,
    elems: impl Iterator<Item = &'a Expr>,
    source: &Expr,
) -> Result<Rc<Term>, String> {
    let mut args = Vec::new();
    for elem in elems {
        let term = translate_term(elem)?;
        if !is_literal_identity_term(term.as_ref()) {
            return Err(format!(
                "{kind} literal contains non-literal element `{}`",
                token_key(source)
            ));
        }
        args.push(term);
    }
    let inner = args
        .iter()
        .map(|arg| canonical_term_sig(arg))
        .collect::<Vec<_>>()
        .join(",");
    Ok(make_var(format!("literal:{kind}({inner})")))
}

fn is_literal_identity_term(term: &Term) -> bool {
    match term {
        Term::Const { .. } => true,
        Term::Var { name } => name.starts_with("literal:"),
        Term::Ctor { name, args } if constructor_operator_tag(term).is_some() => {
            name.starts_with("call:") && args.iter().all(|arg| is_literal_identity_term(arg))
        }
        _ => false,
    }
}

fn type_id_of_call_term(func: &Expr, arg_len: usize) -> Result<Option<Rc<Term>>, String> {
    if arg_len != 0 {
        return Ok(None);
    }
    let Expr::Path(path) = func else {
        return Ok(None);
    };
    if !is_type_id_of_path(&path.path) {
        return Ok(None);
    }
    let Some(last) = path.path.segments.last() else {
        return Ok(None);
    };
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err("TypeId::of requires exactly one type argument".to_string());
    };
    if args.args.len() != 1 {
        return Err("TypeId::of requires exactly one type argument".to_string());
    }
    let Some(syn::GenericArgument::Type(ty)) = args.args.first() else {
        return Err("TypeId::of requires a type argument".to_string());
    };
    Ok(Some(Rc::new(Term::Ctor {
        name: format!("type_id::{}", type_key(ty)),
        args: Vec::new(),
    })))
}

fn is_type_id_of_path(path: &syn::Path) -> bool {
    let segments = path.segments.iter().collect::<Vec<_>>();
    matches!(
        segments.as_slice(),
        [.., type_id, of]
            if type_id.ident == "TypeId" && of.ident == "of"
    )
}

fn is_shared_dyn_any_type(ty: &syn::Type) -> bool {
    let syn::Type::Reference(reference) = ty else {
        return false;
    };
    if reference.mutability.is_some() {
        return false;
    }
    let syn::Type::TraitObject(trait_object) = reference.elem.as_ref() else {
        return false;
    };
    trait_object.bounds.iter().any(|bound| {
        let syn::TypeParamBound::Trait(trait_bound) = bound else {
            return false;
        };
        trait_bound
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Any")
    })
}

fn translate_lit(lit: &ExprLit) -> Result<Rc<Term>, String> {
    match &lit.lit {
        Lit::Int(i) => parse_int_lit(i).map(num),
        Lit::Float(f) => canonical_float_literal(f).map(real_const),
        Lit::Str(s) => Ok(str_const(s.value())),
        Lit::Char(c) => Ok(str_const(c.value().to_string())),
        Lit::Bool(b) => Ok(bool_const(b.value)),
        other => Err(format!(
            "only integer/string/char/finite decimal float scalar constants are liftable, got `{}`",
            token_key(other)
        )),
    }
}

fn parse_int_lit(lit: &syn::LitInt) -> Result<i64, String> {
    let mut text = lit.to_string();
    let suffix = lit.suffix();
    if !suffix.is_empty() && text.ends_with(suffix) {
        text.truncate(text.len() - suffix.len());
    }
    let text = text.replace('_', "");
    let (radix, digits) =
        if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
            (8, rest)
        } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
            (2, rest)
        } else {
            (10, text.as_str())
        };
    i64::from_str_radix(digits, radix).map_err(|e| format!("int literal `{}`: {e}", lit))
}

fn string_or_char_literal_term(expr: &Expr) -> Option<Rc<Term>> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Str(s), ..
        }) => Some(str_const(s.value())),
        Expr::Lit(ExprLit {
            lit: Lit::Char(c), ..
        }) => Some(str_const(c.value().to_string())),
        Expr::Paren(paren) => string_or_char_literal_term(&paren.expr),
        Expr::Group(group) => string_or_char_literal_term(&group.expr),
        _ => None,
    }
}

fn char_literal_term(expr: &Expr) -> Option<Rc<Term>> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Char(c), ..
        }) => Some(str_const(c.value().to_string())),
        Expr::Paren(paren) => char_literal_term(&paren.expr),
        Expr::Group(group) => char_literal_term(&group.expr),
        _ => None,
    }
}

fn canonical_float_literal(lit: &syn::LitFloat) -> Result<String, String> {
    let digits = lit.base10_digits().replace('_', "");
    if digits.contains('e') || digits.contains('E') {
        return Err(format!(
            "float literal with exponent is a refinement gap `{}`",
            lit.to_token_stream()
        ));
    }
    if digits.is_empty() {
        return Err("empty float literal".to_string());
    }
    Ok(digits)
}

fn const_int(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(i), ..
        }) => parse_int_lit(i).ok(),
        Expr::Paren(paren) => const_int(&paren.expr),
        Expr::Group(group) => const_int(&group.expr),
        _ => None,
    }
}

fn term_binop_name(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Add(_) => Some("+"),
        BinOp::Sub(_) => Some("-"),
        BinOp::Mul(_) => Some("*"),
        BinOp::Div(_) => Some("int-div"),
        BinOp::Rem(_) => Some("int-rem"),
        BinOp::BitAnd(_) => Some("bit-and"),
        BinOp::BitOr(_) => Some("bit-or"),
        BinOp::BitXor(_) => Some("bit-xor"),
        BinOp::Shl(_) => Some("shift-left"),
        BinOp::Shr(_) => Some("shift-right"),
        _ => None,
    }
}

fn expr_head_key(expr: &Expr) -> String {
    match expr {
        Expr::Path(path) => path_to_name(&path.path),
        Expr::Paren(paren) => expr_head_key(&paren.expr),
        Expr::Group(group) => expr_head_key(&group.expr),
        other => token_key(other),
    }
}

fn path_to_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| {
            let mut name = segment.ident.to_string();
            name.push_str(&path_arguments_key(&segment.arguments));
            name
        })
        .collect::<Vec<_>>()
        .join("::")
}

fn path_arguments_key(arguments: &syn::PathArguments) -> String {
    match arguments {
        syn::PathArguments::None => String::new(),
        syn::PathArguments::AngleBracketed(args) => angle_args_key(args),
        syn::PathArguments::Parenthesized(args) => token_key(args),
    }
}

fn angle_args_key(args: &syn::AngleBracketedGenericArguments) -> String {
    let inner = args
        .args
        .iter()
        .map(generic_arg_key)
        .collect::<Vec<_>>()
        .join(",");
    format!("::<{inner}>")
}

fn generic_arg_key(arg: &syn::GenericArgument) -> String {
    match arg {
        syn::GenericArgument::Type(ty) => type_key(ty),
        syn::GenericArgument::Const(expr) => format!("const:{}", token_key(expr)),
        syn::GenericArgument::Lifetime(lifetime) => format!("'{}", lifetime.ident),
        syn::GenericArgument::AssocType(assoc) => {
            format!("{}={}", assoc.ident, type_key(&assoc.ty))
        }
        syn::GenericArgument::AssocConst(assoc) => {
            format!("{}=const:{}", assoc.ident, token_key(&assoc.value))
        }
        syn::GenericArgument::Constraint(constraint) => token_key(constraint),
        _ => token_key(arg),
    }
}

fn type_key(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path_to_name(&path.path),
        syn::Type::Reference(reference) => {
            let mut out = String::from("&");
            if let Some(lifetime) = &reference.lifetime {
                out.push('\'');
                out.push_str(&lifetime.ident.to_string());
                out.push(' ');
            }
            if reference.mutability.is_some() {
                out.push_str("mut ");
            }
            out.push_str(&type_key(&reference.elem));
            out
        }
        syn::Type::Tuple(tuple) => {
            let inner = tuple
                .elems
                .iter()
                .map(type_key)
                .collect::<Vec<_>>()
                .join(",");
            format!("({inner})")
        }
        syn::Type::Array(array) => {
            format!("[{};{}]", type_key(&array.elem), token_key(&array.len))
        }
        syn::Type::Slice(slice) => format!("[{}]", type_key(&slice.elem)),
        syn::Type::TraitObject(trait_object) => {
            let bounds = trait_object
                .bounds
                .iter()
                .map(|bound| match bound {
                    syn::TypeParamBound::Trait(trait_bound) => path_to_name(&trait_bound.path),
                    syn::TypeParamBound::Lifetime(lifetime) => format!("'{}", lifetime.ident),
                    _ => token_key(bound),
                })
                .collect::<Vec<_>>()
                .join("+");
            format!("dyn {bounds}")
        }
        _ => token_key(ty),
    }
}

fn token_key<T: ToTokens>(node: T) -> String {
    node.to_token_stream()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
