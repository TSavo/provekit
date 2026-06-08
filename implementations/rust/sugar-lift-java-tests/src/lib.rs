// SPDX-License-Identifier: Apache-2.0
//
// sugar-lift-java-tests
//
// Java/JUnit parity for the rust/python test-assertion consistency path.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use sugar_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use sugar_ir_symbolic::{and_, eq, make_var, num, ContractDecl, Formula, Term};
use sugar_ir_symbolic::{
    atomic_, serialize::marshal_declarations, EvidenceCertificate, EvidenceTerm,
};
use sugar_proof_envelope::{ed25519_pubkey_string, ed25519_sign_string, Ed25519Seed};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertCategory {
    Equality,
    Truth,
    Approx,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertParam {
    pub ty: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedAssert {
    pub owner: String,
    pub name: String,
    pub params: Vec<AssertParam>,
    pub category: AssertCategory,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssertionVocab {
    pub label: String,
    pub methods: Vec<LearnedAssert>,
}

impl AssertionVocab {
    pub fn classify_call(
        &self,
        name: &str,
        arity: usize,
        args: &[String],
    ) -> Option<LearnedAssert> {
        let candidates: Vec<&LearnedAssert> = self
            .methods
            .iter()
            .filter(|m| m.name == name && m.params.len() == arity)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        if args.last().is_some_and(|arg| is_numeric_literal(arg)) {
            if let Some(m) = candidates
                .iter()
                .copied()
                .find(|m| m.category == AssertCategory::Approx)
            {
                return Some(m.clone());
            }
        }
        for category in [
            AssertCategory::Equality,
            AssertCategory::Truth,
            AssertCategory::Approx,
            AssertCategory::Other,
        ] {
            if let Some(m) = candidates.iter().copied().find(|m| m.category == category) {
                return Some(m.clone());
            }
        }
        candidates.first().map(|m| (*m).clone())
    }

    pub fn dump_lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .methods
            .iter()
            .map(|m| {
                let params = m
                    .params
                    .iter()
                    .map(|p| format!("{} {}", p.ty, p.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({}) => {:?}", m.name, params, m.category)
            })
            .collect();
        lines.sort();
        lines
    }
}

pub fn derive_vocab_from_source(
    class_name: &str,
    source: &str,
    exception_dirs: &[String],
) -> Result<AssertionVocab, String> {
    let mut vocab = AssertionVocab {
        label: class_name.to_string(),
        methods: Vec::new(),
    };
    for method in parse_assert_methods(source) {
        let category = classify_assert_method(&method);
        vocab.methods.push(LearnedAssert {
            owner: class_name.to_string(),
            name: method.name,
            params: method.params,
            category,
            source: match category {
                AssertCategory::Approx => "source-signature".to_string(),
                AssertCategory::Equality | AssertCategory::Truth => "source-body".to_string(),
                AssertCategory::Other => "source-opaque".to_string(),
            },
        });
    }
    apply_vocab_exceptions(&mut vocab, class_name, exception_dirs)?;
    Ok(vocab)
}

pub fn learn_vocab_from_exception_dirs(
    class_name: &str,
    source: &str,
    exception_dirs: &[String],
) -> Result<AssertionVocab, String> {
    derive_vocab_from_source(class_name, source, exception_dirs)
}

pub fn learn_vocab_for_test_source(
    source: &str,
    workspace_root: &Path,
    exception_dirs: &[String],
) -> Result<AssertionVocab, String> {
    let classes = scan_assertion_classes(source);
    let mut vocabs = Vec::new();
    for class_name in classes {
        if let Some(path) = resolve_class_source(workspace_root, &class_name) {
            let src = std::fs::read_to_string(&path)
                .map_err(|e| format!("read assertion source {}: {e}", path.display()))?;
            vocabs.push(derive_vocab_from_source(&class_name, &src, exception_dirs)?);
        } else if let Some(cp) = java_assert_classpath(workspace_root) {
            vocabs.push(derive_vocab_from_javap(&class_name, &cp, exception_dirs)?);
        }
    }
    Ok(merge_vocabs(vocabs))
}

fn scan_assertion_classes(source: &str) -> Vec<String> {
    let mut classes = Vec::new();
    let mut simple_imports: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("import static ") {
            let target = rest.trim_end_matches(';').trim();
            if let Some((class, member)) = target.rsplit_once('.') {
                if member == "*" || member.starts_with("assert") {
                    classes.push(class.to_string());
                }
            }
        } else if let Some(rest) = line.strip_prefix("import ") {
            let target = rest.trim_end_matches(';').trim();
            if let Some(simple) = target.rsplit('.').next() {
                simple_imports.push((simple.to_string(), target.to_string()));
            }
        }
    }
    for (simple, class_name) in simple_imports {
        let prefix = format!("{simple}.");
        if source.contains(&format!("{prefix}assert")) {
            classes.push(class_name);
        }
    }
    classes.sort();
    classes.dedup();
    classes
}

fn resolve_class_source(workspace_root: &Path, class_name: &str) -> Option<PathBuf> {
    let rel = class_name.replace('.', "/") + ".java";
    let mut matches = Vec::new();
    for entry in walkdir::WalkDir::new(workspace_root)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && matches!(name.as_ref(), "target" | ".git" | ".sugar"))
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && path.to_string_lossy().replace('\\', "/").ends_with(&rel) {
            matches.push(path.to_path_buf());
        }
    }
    matches.sort();
    matches.into_iter().next()
}

fn java_assert_classpath(workspace_root: &Path) -> Option<String> {
    if let Ok(cp) = std::env::var("SUGAR_JAVA_ASSERT_CLASSPATH") {
        if !cp.trim().is_empty() {
            return Some(cp);
        }
    }
    let path = workspace_root
        .join(".sugar")
        .join("java-assertions")
        .join("classpath");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn derive_vocab_from_javap(
    class_name: &str,
    classpath: &str,
    exception_dirs: &[String],
) -> Result<AssertionVocab, String> {
    let output = Command::new("javap")
        .args(["-classpath", classpath, "-public", class_name])
        .output()
        .map_err(|e| format!("spawn javap for {class_name}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "javap failed for {class_name}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut source_like = String::new();
    source_like.push_str("public final class Reflected {\n");
    for line in text.lines() {
        let line = line.trim().trim_end_matches(';');
        if !line.contains(" assert") && !line.contains(" assert") {
            continue;
        }
        if line.contains('(') && line.contains(')') && line.contains("void ") {
            source_like.push_str("  ");
            source_like.push_str(line);
            source_like.push_str(" {}\n");
        }
    }
    source_like.push_str("}\n");
    let mut vocab = derive_vocab_from_source(class_name, &source_like, &[])?;
    for method in &mut vocab.methods {
        method.source = "javap-signature".to_string();
        for (idx, param) in method.params.iter_mut().enumerate() {
            if param.name.is_empty() {
                param.name = format!("arg{idx}");
            }
        }
    }
    apply_vocab_exceptions(&mut vocab, class_name, exception_dirs)?;
    Ok(vocab)
}

fn merge_vocabs(vocabs: Vec<AssertionVocab>) -> AssertionVocab {
    let mut merged = AssertionVocab {
        label: "auto".to_string(),
        methods: Vec::new(),
    };
    for vocab in vocabs {
        merged.methods.extend(vocab.methods);
    }
    merged
}

pub fn lift_source(src: &str, source_path: &str) -> AdapterOutput {
    lift_source_with_vocab(src, source_path, &AssertionVocab::default())
}

pub fn lift_source_with_vocab(
    src: &str,
    source_path: &str,
    vocab: &AssertionVocab,
) -> AdapterOutput {
    let mut out = AdapterOutput::default();
    let package_name = package_name(src);
    let Some(class_name) = first_class_name(src) else {
        return out;
    };
    for method in test_methods(src) {
        out.seen += 1;
        let test_name = scoped_test_name(
            source_path,
            package_name.as_deref(),
            &class_name,
            &method.name,
        );
        let mut atoms = Vec::new();
        let mut skipped = Vec::new();
        collect_assertion_atoms(&method.body, vocab, &mut atoms, &mut skipped);

        if !skipped.is_empty() {
            out.warnings.push(LiftWarning {
                source_path: source_path.to_string(),
                item_name: test_name,
                reason: format!(
                    "java test assertions: unsupported assertion surface; released to layer 0: {}",
                    skipped.join("; ")
                ),
            });
            continue;
        }
        if atoms.is_empty() {
            out.warnings.push(LiftWarning {
                source_path: source_path.to_string(),
                item_name: test_name,
                reason: "java test assertions: no liftable scalar assertions".to_string(),
            });
            continue;
        }

        out.decls.push(ContractDecl {
            name: test_name,
            pre: None,
            post: None,
            inv: Some(and_(atoms)),
            out_binding: "out".to_string(),
            evidence: None,
            panic_loci: Vec::new(),
            concept_hint: None,
        });
        out.lifted += 1;
    }
    out
}

#[derive(Debug, Clone)]
struct AssertMethodSource {
    name: String,
    params: Vec<AssertParam>,
    body: Option<String>,
}

fn parse_assert_methods(source: &str) -> Vec<AssertMethodSource> {
    let mut out = Vec::new();
    let mut search = 0usize;
    while let Some(open_rel) = source[search..].find('(') {
        let open = search + open_rel;
        let head = &source[..open];
        let tokens = lexical_tokens(head);
        let Some(name) = tokens.last().cloned() else {
            search = open + 1;
            continue;
        };
        if !name.starts_with("assert") {
            search = open + 1;
            continue;
        }
        let signature_tokens = method_signature_tokens(&tokens);
        if !signature_tokens.iter().any(|t| t == "void") {
            search = open + 1;
            continue;
        }
        let Some(close) = matching_brace_like(&source[open..], '(', ')').map(|idx| open + idx)
        else {
            break;
        };
        let params = parse_params(&source[open + 1..close]);
        let mut body = None;
        let mut next_search = close + 1;
        if let Some((offset, ch)) = source[close + 1..]
            .char_indices()
            .find(|(_, ch)| !ch.is_whitespace())
        {
            if ch == '{' {
                let body_open = close + 1 + offset;
                if let Some(body_close) = matching_brace(source, body_open) {
                    body = Some(source[body_open + 1..body_close].to_string());
                    next_search = body_close + 1;
                }
            }
        }
        out.push(AssertMethodSource { name, params, body });
        search = next_search;
    }
    out
}

fn method_signature_tokens(tokens: &[String]) -> Vec<String> {
    let mut start = 0usize;
    for (idx, token) in tokens.iter().enumerate().rev() {
        if matches!(token.as_str(), "public" | "protected" | "private") {
            start = idx;
            break;
        }
    }
    tokens[start..].to_vec()
}

fn parse_params(params_src: &str) -> Vec<AssertParam> {
    split_args(params_src)
        .into_iter()
        .enumerate()
        .filter_map(|(idx, param)| parse_param(&param, idx))
        .collect()
}

fn parse_param(param: &str, idx: usize) -> Option<AssertParam> {
    let mut tokens: Vec<String> = param
        .split_whitespace()
        .filter(|token| {
            !matches!(
                *token,
                "final" | "volatile" | "transient" | "public" | "private" | "protected"
            ) && !token.starts_with('@')
        })
        .map(|token| token.trim_matches(',').to_string())
        .collect();
    if tokens.len() == 1 {
        return Some(AssertParam {
            ty: tokens.pop()?,
            name: format!("arg{idx}"),
        });
    }
    if tokens.len() < 2 {
        return None;
    }
    let name = tokens.pop()?.trim_start_matches("...").to_string();
    let ty = tokens.join(" ").replace("...", "[]");
    Some(AssertParam { ty, name })
}

fn classify_assert_method(method: &AssertMethodSource) -> AssertCategory {
    if carries_tolerance(&method.params) {
        return AssertCategory::Approx;
    }
    if body_delegates_truth(&method.params, method.body.as_deref()) {
        return AssertCategory::Truth;
    }
    if body_delegates_equality(&method.params, method.body.as_deref()) {
        return AssertCategory::Equality;
    }
    AssertCategory::Other
}

fn body_delegates_truth(params: &[AssertParam], body: Option<&str>) -> bool {
    if params.len() != 1 || !is_boolean_type(&params[0].ty) {
        return false;
    }
    let Some(body) = body else {
        return false;
    };
    let p = &params[0].name;
    let c = compact_no_ws(body);
    c.contains(&format!("!{p}"))
        || c.contains(&format!("{p}==false"))
        || c.contains(&format!("false=={p}"))
        || c.contains(&format!("{p}!=true"))
        || c.contains(&format!("true!={p}"))
}

fn body_delegates_equality(params: &[AssertParam], body: Option<&str>) -> bool {
    if params.len() < 2 {
        return false;
    }
    let Some(body) = body else {
        return false;
    };
    let a = &params[0].name;
    let b = &params[1].name;
    let c = compact_no_ws(body);
    let patterns = [
        format!("{a}!={b}"),
        format!("{b}!={a}"),
        format!("{a}=={b}"),
        format!("{b}=={a}"),
        format!("Objects.equals({a},{b})"),
        format!("Objects.equals({b},{a})"),
        format!("java.util.Objects.equals({a},{b})"),
        format!("java.util.Objects.equals({b},{a})"),
        format!("{a}.equals({b})"),
        format!("{b}.equals({a})"),
    ];
    patterns.iter().any(|p| c.contains(p))
}

fn compact_no_ws(s: &str) -> String {
    s.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn carries_tolerance(params: &[AssertParam]) -> bool {
    const TOL_NAMES: &[&str] = &[
        "rtol",
        "atol",
        "decimal",
        "significant",
        "nulp",
        "maxulp",
        "places",
        "delta",
        "epsilon",
        "eps",
        "tolerance",
        "tol",
    ];
    if params.iter().any(|p| {
        let n = p.name.to_ascii_lowercase();
        TOL_NAMES.contains(&n.as_str())
    }) {
        return true;
    }
    for idx in 2..params.len() {
        if !is_floating_type(&params[idx].ty) {
            continue;
        }
        if params[..idx]
            .iter()
            .filter(|p| is_floating_comparable_type(&p.ty))
            .count()
            < 2
        {
            continue;
        }
        if params[idx + 1..].iter().all(|p| is_message_type(&p.ty)) {
            return true;
        }
    }
    false
}

fn is_floating_type(ty: &str) -> bool {
    matches!(ty.trim(), "double" | "Double" | "float" | "Float")
}

fn is_floating_comparable_type(ty: &str) -> bool {
    matches!(
        ty.trim(),
        "double" | "Double" | "float" | "Float" | "double[]" | "Double[]" | "float[]" | "Float[]"
    )
}

fn is_message_type(ty: &str) -> bool {
    let ty = ty.trim();
    matches!(ty, "String" | "java.lang.String")
        || ty == "Supplier<String>"
        || ty == "java.util.function.Supplier<String>"
        || ty == "java.util.function.Supplier<java.lang.String>"
}

fn is_boolean_type(ty: &str) -> bool {
    matches!(ty.trim(), "boolean" | "Boolean")
}

fn apply_vocab_exceptions(
    vocab: &mut AssertionVocab,
    class_name: &str,
    exception_dirs: &[String],
) -> Result<(), String> {
    let Some(exc) = load_exception(class_name, exception_dirs)? else {
        return Ok(());
    };
    let overrides = exc
        .get("overrides")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for (category, names) in overrides {
        let Some(category) = category_from_str(&category) else {
            continue;
        };
        let Some(names) = names.as_array() else {
            continue;
        };
        for name in names.iter().filter_map(|v| v.as_str()) {
            let mut found = false;
            for method in &mut vocab.methods {
                if method.name == name {
                    found = true;
                    if matches!(method.category, AssertCategory::Approx) {
                        continue;
                    }
                    method.category = category;
                    method.source = "external-exception".to_string();
                }
            }
            if !found {
                vocab.methods.push(LearnedAssert {
                    owner: class_name.to_string(),
                    name: name.to_string(),
                    params: Vec::new(),
                    category,
                    source: "external-exception".to_string(),
                });
            }
        }
    }
    Ok(())
}

fn category_from_str(s: &str) -> Option<AssertCategory> {
    match s {
        "equality" => Some(AssertCategory::Equality),
        "truth" => Some(AssertCategory::Truth),
        "approx" => Some(AssertCategory::Approx),
        "other" => Some(AssertCategory::Other),
        _ => None,
    }
}

fn load_exception(
    class_name: &str,
    exception_dirs: &[String],
) -> Result<Option<serde_json::Value>, String> {
    for dir in exception_dirs {
        let path = Path::new(dir).join(format!("{class_name}.json"));
        if path.is_file() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("read vocab exception {}: {e}", path.display()))?;
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| format!("parse vocab exception {}: {e}", path.display()))?;
            return Ok(Some(value));
        }
    }
    Ok(None)
}

#[derive(Debug, Clone)]
struct JavaMethod {
    name: String,
    body: String,
}

fn package_name(src: &str) -> Option<String> {
    src.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("package ")?;
        Some(rest.trim_end_matches(';').trim().to_string())
    })
}

fn first_class_name(src: &str) -> Option<String> {
    let tokens = lexical_tokens(src);
    for pair in tokens.windows(2) {
        if matches!(pair[0].as_str(), "class" | "record") {
            return Some(pair[1].clone());
        }
    }
    None
}

fn scoped_test_name(source_path: &str, package: Option<&str>, class: &str, method: &str) -> String {
    let class_key = match package {
        Some(pkg) if !pkg.is_empty() => format!("{pkg}.{class}"),
        _ => class.to_string(),
    };
    format!("{source_path}::{class_key}.{method}")
}

fn test_methods(src: &str) -> Vec<JavaMethod> {
    let mut methods = Vec::new();
    let mut search = 0;
    while let Some(at) = find_annotation(src, search) {
        let after = at + src[at..].find("@Test").unwrap_or(0) + "@Test".len();
        let Some(open) = src[after..].find('{').map(|idx| after + idx) else {
            break;
        };
        let signature = &src[after..open];
        let Some(name) = method_name_from_signature(signature) else {
            search = after;
            continue;
        };
        let Some(close) = matching_brace(src, open) else {
            break;
        };
        methods.push(JavaMethod {
            name,
            body: src[open + 1..close].to_string(),
        });
        search = close + 1;
    }
    methods
}

fn find_annotation(src: &str, start: usize) -> Option<usize> {
    let mut pos = start;
    loop {
        let found = src[pos..].find("@Test").map(|idx| pos + idx)?;
        let after = found + "@Test".len();
        let boundary = src[after..]
            .chars()
            .next()
            .is_none_or(|ch| !is_ident_char(ch));
        if boundary {
            return Some(found);
        }
        pos = after;
    }
}

fn method_name_from_signature(signature: &str) -> Option<String> {
    let before_paren = signature.rsplit_once('(')?.0;
    lexical_tokens(before_paren)
        .into_iter()
        .last()
        .filter(|token| token != "Test")
}

fn matching_brace(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (offset, ch) in src[open..].char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn lexical_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in s.chars() {
        if is_ident_char(ch) || ch == '.' {
            buf.push(ch);
        } else if !buf.is_empty() {
            out.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

fn is_ident_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
}

fn collect_assertion_atoms(
    body: &str,
    vocab: &AssertionVocab,
    atoms: &mut Vec<Rc<Formula>>,
    skipped: &mut Vec<String>,
) {
    for stmt in split_statements(body) {
        match assertion_from_statement(&stmt, vocab) {
            Ok(Some(atom)) => atoms.push(atom),
            Ok(None) => {}
            Err(reason) => skipped.push(reason),
        }
    }
}

fn split_statements(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut paren = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in body.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => paren += 1,
            ')' => paren -= 1,
            ';' if paren == 0 => {
                let stmt = body[start..idx].trim();
                if !stmt.is_empty() {
                    out.push(stmt.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    out
}

fn assertion_from_statement(
    stmt: &str,
    vocab: &AssertionVocab,
) -> Result<Option<Rc<Formula>>, String> {
    let Some((head, args_src)) = call_parts(stmt) else {
        return Ok(None);
    };
    let name = head.rsplit('.').next().unwrap_or(head).trim();
    let args = split_args(args_src);
    let Some(method) = vocab.classify_call(name, args.len(), &args) else {
        if name.starts_with("assert") {
            return Err(format!(
                "LOUD REFUSAL -- assertion `{name}` has no learned vocabulary entry"
            ));
        }
        return Ok(None);
    };
    match method.category {
        AssertCategory::Equality => {
            if args.len() < 2 {
                return Err(format!("{name}: expected at least 2 arguments"));
            }
            let expected = translate_term(&args[0]).map_err(|e| format!("{name}: {e}"))?;
            let actual = translate_term(&args[1]).map_err(|e| format!("{name}: {e}"))?;
            Ok(Some(eq(actual, expected)))
        }
        AssertCategory::Truth => {
            let Some(first) = args.first() else {
                return Err(format!("{name}: expected a condition"));
            };
            let atom = translate_bool_assertion(first).map_err(|e| format!("{name}: {e}"))?;
            Ok(Some(atom))
        }
        AssertCategory::Approx => Err(format!(
            "approximate assertion `{name}` is not exact equality (a ~= b within tolerance); refused to avoid false-pass"
        )),
        AssertCategory::Other => Err(format!(
            "LOUD REFUSAL -- assertion `{name}` is not structurally clear; not lifted"
        )),
    }
}

fn call_parts(stmt: &str) -> Option<(&str, &str)> {
    let open = stmt.find('(')?;
    let close = stmt.rfind(')')?;
    if close <= open {
        return None;
    }
    Some((stmt[..open].trim(), &stmt[open + 1..close]))
}

fn split_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut paren = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in args.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => paren += 1,
            ')' => paren -= 1,
            ',' if paren == 0 => {
                out.push(args[start..idx].trim().to_string());
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn translate_bool_assertion(expr: &str) -> Result<Rc<Formula>, String> {
    let Some((lhs, rhs)) = split_top_level_eq(expr) else {
        return Err(format!(
            "only scalar equality is liftable, got `{}`",
            compact(expr)
        ));
    };
    Ok(eq(translate_term(lhs)?, translate_term(rhs)?))
}

fn split_top_level_eq(expr: &str) -> Option<(&str, &str)> {
    let mut paren = 0i32;
    let bytes = expr.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        match bytes[i] as char {
            '(' => paren += 1,
            ')' => paren -= 1,
            '=' if paren == 0 && bytes[i + 1] == b'=' => {
                return Some((expr[..i].trim(), expr[i + 2..].trim()));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn translate_term(expr: &str) -> Result<Rc<Term>, String> {
    let expr = trim_wrapping_parens(expr.trim());
    if let Ok(value) = expr.parse::<i64>() {
        return Ok(num(value));
    }
    if is_numeric_literal(expr) {
        return Ok(make_var(compact(expr)));
    }
    if let Some(rest) = expr.strip_prefix('-') {
        if let Ok(value) = rest.trim().parse::<i64>() {
            return Ok(num(-value));
        }
    }
    if let Some((lhs, op, rhs)) = split_top_level_arithmetic(expr) {
        return Ok(Rc::new(Term::Ctor {
            name: op.to_string(),
            args: vec![translate_term(lhs)?, translate_term(rhs)?],
        }));
    }
    if let Some((head, args_src)) = call_parts(expr) {
        let args = split_args(args_src)
            .iter()
            .map(|arg| translate_term(arg))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Rc::new(Term::Ctor {
            name: format!("call:{}", compact(head)),
            args,
        }));
    }
    if is_java_path(expr) {
        return Ok(make_var(compact(expr)));
    }
    Err(format!("unsupported term `{}`", compact(expr)))
}

fn split_top_level_arithmetic(expr: &str) -> Option<(&str, &'static str, &str)> {
    for op in ['+', '-', '*'] {
        let mut paren = 0i32;
        for (idx, ch) in expr.char_indices().rev() {
            match ch {
                ')' => paren += 1,
                '(' => paren -= 1,
                _ if paren == 0 && ch == op && idx > 0 => {
                    let lhs = expr[..idx].trim();
                    let rhs = expr[idx + ch.len_utf8()..].trim();
                    if !lhs.is_empty() && !rhs.is_empty() {
                        return Some((
                            lhs,
                            match op {
                                '+' => "+",
                                '-' => "-",
                                '*' => "*",
                                _ => unreachable!(),
                            },
                            rhs,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn trim_wrapping_parens(mut expr: &str) -> &str {
    loop {
        let s = expr.trim();
        if s.starts_with('(')
            && s.ends_with(')')
            && matching_brace_like(s, '(', ')').is_some_and(|idx| idx == s.len() - 1)
        {
            expr = &s[1..s.len() - 1];
        } else {
            return s;
        }
    }
}

fn matching_brace_like(src: &str, open_ch: char, close_ch: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in src.char_indices() {
        if ch == open_ch {
            depth += 1;
        } else if ch == close_ch {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn is_java_path(expr: &str) -> bool {
    !expr.is_empty()
        && expr.chars().all(|ch| is_ident_char(ch) || ch == '.')
        && expr
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphabetic())
}

fn compact(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_numeric_literal(expr: &str) -> bool {
    let s = expr.trim().trim_end_matches(['f', 'F', 'd', 'D']);
    s.parse::<f64>().is_ok()
}

// ---------------------------------------------------------------------------
// JUnit witness package: Java parity for rust-cargo-test-witness.
// ---------------------------------------------------------------------------

pub const WITNESS_SIGNER_SEED: Ed25519Seed = [0x77u8; 32];
const SIGNER_SEED_ENV: &str = "SUGAR_WITNESS_SIGNER_SEED";
const JUNIT_JAR_ENV: &str = "SUGAR_JUNIT_CONSOLE_JAR";
const TESTNG_CLASSPATH_ENV: &str = "SUGAR_TESTNG_CLASSPATH";
const JUNIT_TEST_WITNESS_KIND: &str = "junit-test-witness";
const JUNIT_PACKAGE_KIND: &str = "junit-test-witness-package";
const TESTNG_TEST_WITNESS_KIND: &str = "testng-test-witness";
const TESTNG_PACKAGE_KIND: &str = "testng-test-witness-package";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTest {
    pub test_id: String,
    pub raw: String, // "passed" | "failed" | "skipped"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Witness {
    pub kind: String,
    pub code_cid: String,
    pub runtime_cid: String,
    pub test_id: String,
    pub outcome: String,
    pub code_files: Vec<String>,
    pub cid: String,
}

impl Witness {
    pub fn new_for_test(
        code: &str,
        runtime: &str,
        test_id: &str,
        outcome: &str,
        code_files: &[String],
    ) -> Self {
        make_witness(
            JUNIT_TEST_WITNESS_KIND,
            code,
            runtime,
            test_id,
            outcome,
            code_files,
        )
    }
}

fn resolve_signer_seed(seed: Option<Ed25519Seed>) -> Result<Ed25519Seed, String> {
    if let Some(s) = seed {
        return Ok(s);
    }
    if let Ok(env) = std::env::var(SIGNER_SEED_ENV) {
        let env = env.trim();
        if !env.is_empty() {
            let raw = decode_hex(env)?;
            if raw.len() != 32 {
                return Err(format!(
                    "{SIGNER_SEED_ENV} must be 64 hex chars (32 bytes); got {}",
                    raw.len()
                ));
            }
            let mut out = [0u8; 32];
            out.copy_from_slice(&raw);
            return Ok(out);
        }
    }
    Ok(WITNESS_SIGNER_SEED)
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd-length hex".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

pub fn code_cid(project_dir: &Path, code_files: &[String]) -> Result<String, String> {
    let mut sorted = code_files.to_vec();
    sorted.sort();
    let mut parts: Vec<Vec<u8>> = Vec::new();
    for rel in &sorted {
        let bytes = std::fs::read(project_dir.join(rel))
            .map_err(|e| format!("read code file {rel}: {e}"))?;
        let mut part = rel.as_bytes().to_vec();
        part.push(0u8);
        part.extend_from_slice(&bytes);
        parts.push(part);
    }
    Ok(blake3_512_of(&join_with_nul(&parts)))
}

fn join_with_nul(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(0u8);
        }
        out.extend_from_slice(p);
    }
    out
}

pub fn runtime_cid() -> String {
    runtime_cid_for("junit", JUNIT_JAR_ENV)
}

pub fn testng_runtime_cid() -> String {
    runtime_cid_for("testng", TESTNG_CLASSPATH_ENV)
}

fn runtime_cid_for(tool: &str, classpath_env: &str) -> String {
    let javac = command_version("javac", &["-version"]);
    let java = command_version("java", &["-version"]);
    let classpath =
        std::env::var(classpath_env).unwrap_or_else(|_| format!("{tool}-classpath-unknown"));
    let desc = format!("javac={javac};java={java};{tool}={classpath}");
    blake3_512_of(desc.as_bytes())
}

fn command_version(bin: &str, args: &[&str]) -> String {
    Command::new(bin)
        .args(args)
        .output()
        .ok()
        .map(|o| {
            let mut s = String::new();
            s.push_str(&String::from_utf8_lossy(&o.stdout));
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            s.trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{bin}-unknown"))
}

fn witness_value(
    kind: &str,
    code: &str,
    runtime: &str,
    test_id: &str,
    outcome: &str,
    code_files: &[String],
) -> std::sync::Arc<CValue> {
    let mut files = code_files.to_vec();
    files.sort();
    CValue::object([
        ("kind", CValue::string(kind.to_string())),
        ("codeCid", CValue::string(code.to_string())),
        ("codeFiles", CValue::string(files.join(","))),
        ("outcome", CValue::string(outcome.to_string())),
        ("runtimeCid", CValue::string(runtime.to_string())),
        ("test", CValue::string(test_id.to_string())),
    ])
}

pub fn witness_body(w: &Witness) -> Vec<u8> {
    encode_jcs(&witness_value(
        &w.kind,
        &w.code_cid,
        &w.runtime_cid,
        &w.test_id,
        &w.outcome,
        &w.code_files,
    ))
    .into_bytes()
}

fn make_witness(
    kind: &str,
    code: &str,
    runtime: &str,
    test_id: &str,
    outcome: &str,
    code_files: &[String],
) -> Witness {
    let mut files = code_files.to_vec();
    files.sort();
    let body = encode_jcs(&witness_value(
        kind, code, runtime, test_id, outcome, &files,
    ));
    let cid = blake3_512_of(body.as_bytes());
    Witness {
        kind: kind.to_string(),
        code_cid: code.to_string(),
        runtime_cid: runtime.to_string(),
        test_id: test_id.to_string(),
        outcome: outcome.to_string(),
        code_files: files,
        cid,
    }
}

pub fn parse_junit_xml_reports(xml: &str) -> Vec<ParsedTest> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(found) = xml[pos..].find("<testcase") {
        let start = pos + found;
        let Some(tag_end) = xml[start..].find('>').map(|idx| start + idx) else {
            break;
        };
        let tag = &xml[start..=tag_end];
        let classname = xml_attr(tag, "classname").unwrap_or_default();
        let raw_name = xml_attr(tag, "name").unwrap_or_default();
        let name = raw_name.trim_end_matches("()").to_string();
        let self_closing = tag.trim_end().ends_with("/>");
        let mut raw = "passed";
        let next_pos = if self_closing {
            tag_end + 1
        } else if let Some(close) = xml[tag_end + 1..].find("</testcase>") {
            let body_end = tag_end + 1 + close;
            let body = &xml[tag_end + 1..body_end];
            if body.contains("<skipped") {
                raw = "skipped";
            } else if body.contains("<failure") || body.contains("<error") {
                raw = "failed";
            }
            body_end + "</testcase>".len()
        } else {
            tag_end + 1
        };
        if !name.is_empty() {
            let test_id = if classname.is_empty() {
                name
            } else {
                format!("{classname}.{name}")
            };
            out.push(ParsedTest {
                test_id,
                raw: raw.to_string(),
            });
        }
        pos = next_pos;
    }
    out
}

fn xml_attr(tag: &str, attr: &str) -> Option<String> {
    let pat = format!("{attr}=\"");
    let start = tag.find(&pat)? + pat.len();
    let end = tag[start..].find('"')? + start;
    Some(xml_unescape(&tag[start..end]))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn witnesses_from_parsed(
    kind: &str,
    parsed: &[ParsedTest],
    cc: &str,
    rc: &str,
    code_files: &[String],
) -> Vec<Witness> {
    parsed
        .iter()
        .filter(|p| p.raw != "skipped")
        .map(|p| {
            let outcome = if p.raw == "passed" {
                "passed"
            } else {
                "failed"
            };
            make_witness(kind, cc, rc, &p.test_id, outcome, code_files)
        })
        .collect()
}

pub fn build_bundle(witnesses: &[Witness]) -> (Vec<u8>, String, Vec<Witness>) {
    let mut ws = witnesses.to_vec();
    ws.sort_by(|a, b| a.test_id.cmp(&b.test_id));
    let mut buf = Vec::new();
    for w in &ws {
        buf.extend_from_slice(&witness_body(w));
        buf.push(b'\n');
    }
    let cid = blake3_512_of(&buf);
    (buf, cid, ws)
}

pub fn discover_java_files(project_dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut code_files = Vec::new();
    for entry in walkdir::WalkDir::new(project_dir)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir() && matches!(name.as_ref(), "target" | ".git" | ".sugar"))
        })
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "java") {
            if let Ok(rel) = path.strip_prefix(project_dir) {
                code_files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    code_files.sort();
    (code_files, vec![".".to_string()])
}

pub fn run_suite_witnesses(
    project_dir: &Path,
    code_files: &[String],
) -> Result<Vec<Witness>, String> {
    let cc = code_cid(project_dir, code_files)?;
    let rc = runtime_cid();
    let parsed = run_junit_suite(project_dir)?;
    Ok(witnesses_from_parsed(
        JUNIT_TEST_WITNESS_KIND,
        &parsed,
        &cc,
        &rc,
        code_files,
    ))
}

pub fn build_suite_bundle(
    project_dir: &Path,
    code_files: &[String],
) -> Result<(Vec<u8>, String, Vec<Witness>), String> {
    let witnesses = run_suite_witnesses(project_dir, code_files)?;
    Ok(build_bundle(&witnesses))
}

pub fn run_testng_suite_witnesses(
    project_dir: &Path,
    code_files: &[String],
) -> Result<Vec<Witness>, String> {
    let cc = code_cid(project_dir, code_files)?;
    let rc = testng_runtime_cid();
    let parsed = run_testng_suite(project_dir)?;
    Ok(witnesses_from_parsed(
        TESTNG_TEST_WITNESS_KIND,
        &parsed,
        &cc,
        &rc,
        code_files,
    ))
}

pub fn build_testng_suite_bundle(
    project_dir: &Path,
    code_files: &[String],
) -> Result<(Vec<u8>, String, Vec<Witness>), String> {
    let witnesses = run_testng_suite_witnesses(project_dir, code_files)?;
    Ok(build_bundle(&witnesses))
}

fn junit_console_jar() -> Result<PathBuf, String> {
    let jar = std::env::var(JUNIT_JAR_ENV)
        .map_err(|_| format!("{JUNIT_JAR_ENV} is not set; cannot run JUnit witness"))?;
    let path = PathBuf::from(jar);
    if !path.is_file() {
        return Err(format!(
            "JUnit console jar does not exist: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn run_junit_suite(project_dir: &Path) -> Result<Vec<ParsedTest>, String> {
    let jar = junit_console_jar()?;
    let (java_files, _) = discover_java_files(project_dir);
    if java_files.is_empty() {
        return Err("no Java source files found".to_string());
    }
    let work = project_dir.join("target").join("sugar-java-junit");
    let classes = work.join("classes");
    let reports = work.join("reports");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&classes).map_err(|e| format!("mkdir classes: {e}"))?;
    std::fs::create_dir_all(&reports).map_err(|e| format!("mkdir reports: {e}"))?;

    let mut javac = Command::new("javac");
    javac.arg("-cp").arg(&jar).arg("-d").arg(&classes);
    for rel in &java_files {
        javac.arg(project_dir.join(rel));
    }
    let javac_out = javac.output().map_err(|e| format!("spawn javac: {e}"))?;
    if !javac_out.status.success() {
        return Err(format!(
            "javac failed: {}{}",
            String::from_utf8_lossy(&javac_out.stdout),
            String::from_utf8_lossy(&javac_out.stderr)
        ));
    }

    let output = Command::new("java")
        .arg("-jar")
        .arg(&jar)
        .arg("execute")
        .arg("--class-path")
        .arg(&classes)
        .arg("--scan-class-path")
        .arg("--reports-dir")
        .arg(&reports)
        .arg("--details=none")
        .output()
        .map_err(|e| format!("spawn junit console: {e}"))?;
    let mut parsed = parse_report_dir(&reports)?;
    if parsed.is_empty() {
        return Err(format!(
            "JUnit produced no test cases; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parsed.sort_by(|a, b| a.test_id.cmp(&b.test_id));
    Ok(parsed)
}

fn testng_classpath() -> Result<String, String> {
    let cp = std::env::var(TESTNG_CLASSPATH_ENV)
        .map_err(|_| format!("{TESTNG_CLASSPATH_ENV} is not set; cannot run TestNG witness"))?;
    if cp.trim().is_empty() {
        return Err(format!("{TESTNG_CLASSPATH_ENV} is empty"));
    }
    Ok(cp)
}

fn run_testng_suite(project_dir: &Path) -> Result<Vec<ParsedTest>, String> {
    let cp = testng_classpath()?;
    let (java_files, _) = discover_java_files(project_dir);
    if java_files.is_empty() {
        return Err("no Java source files found".to_string());
    }
    let test_classes = discover_test_classes(project_dir, &java_files)?;
    if test_classes.is_empty() {
        return Err("no TestNG @Test classes found".to_string());
    }

    let work = project_dir.join("target").join("sugar-java-testng");
    let classes = work.join("classes");
    let reports = work.join("reports");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&classes).map_err(|e| format!("mkdir classes: {e}"))?;
    std::fs::create_dir_all(&reports).map_err(|e| format!("mkdir reports: {e}"))?;

    let mut javac = Command::new("javac");
    javac.arg("-cp").arg(&cp).arg("-d").arg(&classes);
    for rel in &java_files {
        javac.arg(project_dir.join(rel));
    }
    let javac_out = javac.output().map_err(|e| format!("spawn javac: {e}"))?;
    if !javac_out.status.success() {
        return Err(format!(
            "javac failed: {}{}",
            String::from_utf8_lossy(&javac_out.stdout),
            String::from_utf8_lossy(&javac_out.stderr)
        ));
    }

    let runtime_cp = format!("{cp}:{}", classes.display());
    let output = Command::new("java")
        .arg("-cp")
        .arg(runtime_cp)
        .arg("org.testng.TestNG")
        .arg("-testclass")
        .arg(test_classes.join(","))
        .arg("-d")
        .arg(&reports)
        .output()
        .map_err(|e| format!("spawn TestNG: {e}"))?;
    let mut parsed = parse_report_dir(&reports)?;
    if parsed.is_empty() {
        return Err(format!(
            "TestNG produced no test cases; stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parsed.sort_by(|a, b| a.test_id.cmp(&b.test_id));
    Ok(parsed)
}

fn discover_test_classes(project_dir: &Path, java_files: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for rel in java_files {
        let src = std::fs::read_to_string(project_dir.join(rel))
            .map_err(|e| format!("read Java source {rel}: {e}"))?;
        if !src.contains("@Test") {
            continue;
        }
        let Some(class_name) = first_class_name(&src) else {
            continue;
        };
        if let Some(package) = package_name(&src) {
            out.push(format!("{package}.{class_name}"));
        } else {
            out.push(class_name);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn parse_report_dir(reports: &Path) -> Result<Vec<ParsedTest>, String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(reports)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "xml") {
            let xml = std::fs::read_to_string(&path)
                .map_err(|e| format!("read Java test report {}: {e}", path.display()))?;
            out.extend(parse_junit_xml_reports(&xml));
        }
    }
    Ok(out)
}

pub fn witness_package_memento(
    bundle_cid: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
    seed: Option<Ed25519Seed>,
) -> Result<serde_json::Value, String> {
    witness_package_memento_for(
        JUNIT_PACKAGE_KIND,
        bundle_cid,
        test_files,
        code_files,
        count,
        passed,
        seed,
    )
}

pub fn testng_witness_package_memento(
    bundle_cid: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
    seed: Option<Ed25519Seed>,
) -> Result<serde_json::Value, String> {
    witness_package_memento_for(
        TESTNG_PACKAGE_KIND,
        bundle_cid,
        test_files,
        code_files,
        count,
        passed,
        seed,
    )
}

fn witness_package_memento_for(
    witness_kind: &str,
    bundle_cid: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
    seed: Option<Ed25519Seed>,
) -> Result<serde_json::Value, String> {
    let seed = resolve_signer_seed(seed)?;
    let signer = ed25519_pubkey_string(&seed);
    let signature = ed25519_sign_string(&seed, bundle_cid.as_bytes());
    let mut tfs = test_files.to_vec();
    tfs.sort();
    let mut cfs = code_files.to_vec();
    cfs.sort();
    Ok(serde_json::json!({
        "kind": "witness-memento",
        "witness_cid": bundle_cid,
        "witness_kind": witness_kind,
        "signer": signer,
        "signature": signature,
        "test_files": tfs,
        "code_files": cfs,
        "count": count,
        "passed": passed,
    }))
}

pub fn witness_package_proof_data(
    bundle_cid: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
) -> String {
    let mut tfs = test_files.to_vec();
    tfs.sort();
    let mut cfs = code_files.to_vec();
    cfs.sort();
    let v = serde_json::json!({
        "kind": "witness-package",
        "packageCid": bundle_cid,
        "testFiles": tfs,
        "codeFiles": cfs,
        "count": count,
        "passed": passed,
    });
    sorted_compact_json(&v)
}

fn sorted_compact_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut s = String::from("{");
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&serde_json::to_string(k).unwrap());
                s.push(':');
                s.push_str(&sorted_compact_json(&map[*k]));
            }
            s.push('}');
            s
        }
        serde_json::Value::Array(items) => {
            let mut s = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&sorted_compact_json(item));
            }
            s.push(']');
            s
        }
        other => serde_json::to_string(other).unwrap(),
    }
}

pub fn junit_witness_package_contract_ir(
    bundle_cid: &str,
    runtime: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
) -> serde_json::Value {
    witness_package_contract_ir_for(
        "junit", bundle_cid, runtime, test_files, code_files, count, passed,
    )
}

pub fn testng_witness_package_contract_ir(
    bundle_cid: &str,
    runtime: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
) -> serde_json::Value {
    witness_package_contract_ir_for(
        "testng", bundle_cid, runtime, test_files, code_files, count, passed,
    )
}

fn witness_package_contract_ir_for(
    tool: &str,
    bundle_cid: &str,
    runtime: &str,
    test_files: &[String],
    code_files: &[String],
    count: usize,
    passed: usize,
) -> serde_json::Value {
    let proof_data = witness_package_proof_data(bundle_cid, test_files, code_files, count, passed);
    let cert = EvidenceCertificate {
        tool: tool.to_string(),
        version: runtime.to_string(),
        formula_hash: bundle_cid.to_string(),
        proof_data,
    };
    let ev = EvidenceTerm {
        proof_type: "custom".to_string(),
        certificate: cert,
    };
    let decl = ContractDecl {
        name: format!("witness-package:{bundle_cid}"),
        pre: None,
        post: None,
        inv: Some(atomic_("witnessed", vec![])),
        out_binding: "out".to_string(),
        evidence: Some(ev),
        panic_loci: Vec::new(),
        concept_hint: None,
    };
    let arr: serde_json::Value =
        serde_json::from_str(&marshal_declarations(&[decl])).expect("contract marshals to JSON");
    arr.as_array()
        .and_then(|a| a.first())
        .cloned()
        .expect("one contract member")
}

pub fn cid_filename(cid: &str, ext: &str) -> String {
    format!("{}{ext}", cid.replace(':', "_"))
}

pub fn write_bundle_package(
    project_dir: &Path,
    bundle_cid: &str,
    bundle_bytes: &[u8],
) -> Result<PathBuf, String> {
    let dir = project_dir.join(".sugar").join("witnesses");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir witnesses: {e}"))?;
    let path = dir.join(cid_filename(bundle_cid, ".witness"));
    std::fs::write(&path, bundle_bytes).map_err(|e| format!("write witness package: {e}"))?;
    Ok(path)
}

pub struct LiftProjectResult {
    pub ir: Vec<serde_json::Value>,
    pub mementos: Vec<serde_json::Value>,
    pub bundle_cid: String,
    pub bundle_bytes: Vec<u8>,
}

pub fn lift_project(project_dir: &Path) -> Result<Option<LiftProjectResult>, String> {
    let (code_files, test_files) = discover_java_files(project_dir);
    if code_files.is_empty() {
        return Ok(None);
    }
    let (bundle_bytes, bundle_cid, witnesses) = build_suite_bundle(project_dir, &code_files)?;
    if witnesses.is_empty() {
        return Ok(None);
    }
    let count = witnesses.len();
    let passed = witnesses.iter().filter(|w| w.outcome == "passed").count();
    let runtime = runtime_cid();
    let memento =
        witness_package_memento(&bundle_cid, &test_files, &code_files, count, passed, None)?;
    let contract = junit_witness_package_contract_ir(
        &bundle_cid,
        &runtime,
        &test_files,
        &code_files,
        count,
        passed,
    );
    Ok(Some(LiftProjectResult {
        ir: vec![contract],
        mementos: vec![memento],
        bundle_cid,
        bundle_bytes,
    }))
}

pub fn lift_testng_project(project_dir: &Path) -> Result<Option<LiftProjectResult>, String> {
    let (code_files, test_files) = discover_java_files(project_dir);
    if code_files.is_empty() {
        return Ok(None);
    }
    let (bundle_bytes, bundle_cid, witnesses) =
        build_testng_suite_bundle(project_dir, &code_files)?;
    if witnesses.is_empty() {
        return Ok(None);
    }
    let count = witnesses.len();
    let passed = witnesses.iter().filter(|w| w.outcome == "passed").count();
    let runtime = testng_runtime_cid();
    let memento =
        testng_witness_package_memento(&bundle_cid, &test_files, &code_files, count, passed, None)?;
    let contract = testng_witness_package_contract_ir(
        &bundle_cid,
        &runtime,
        &test_files,
        &code_files,
        count,
        passed,
    );
    Ok(Some(LiftProjectResult {
        ir: vec![contract],
        mementos: vec![memento],
        bundle_cid,
        bundle_bytes,
    }))
}

pub fn parse_evidence_proof_data(evidence_json: &str) -> Result<serde_json::Value, String> {
    let ev: serde_json::Value =
        serde_json::from_str(evidence_json).map_err(|e| format!("parse evidence JSON: {e}"))?;
    let proof_data = ev
        .get("certificate")
        .and_then(|c| c.get("proofData"))
        .and_then(|v| v.as_str())
        .ok_or("evidence missing certificate.proofData")?;
    serde_json::from_str(proof_data).map_err(|e| format!("parse proofData: {e}"))
}

pub fn memento_str_list(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn discharge_bundle(
    bundle_cid: &str,
    code_files: &[String],
    project_dir: &Path,
) -> (String, String) {
    discharge_bundle_with(
        "JUnit",
        bundle_cid,
        code_files,
        project_dir,
        build_suite_bundle,
    )
}

pub fn discharge_testng_bundle(
    bundle_cid: &str,
    code_files: &[String],
    project_dir: &Path,
) -> (String, String) {
    discharge_bundle_with(
        "TestNG",
        bundle_cid,
        code_files,
        project_dir,
        build_testng_suite_bundle,
    )
}

fn discharge_bundle_with(
    label: &str,
    bundle_cid: &str,
    code_files: &[String],
    project_dir: &Path,
    build: fn(&Path, &[String]) -> Result<(Vec<u8>, String, Vec<Witness>), String>,
) -> (String, String) {
    let (_, cid, witnesses) = match build(project_dir, code_files) {
        Ok(t) => t,
        Err(e) => return ("REFUSED".to_string(), format!("discharge error: {e}")),
    };
    let n = witnesses.len();
    if cid != bundle_cid {
        return (
            "REFUSED".to_string(),
            format!(
                "suite did not reproduce the pinned bundle: recomputed {}... != pinned {}... ({n} tests re-run)",
                short(&cid),
                short(bundle_cid)
            ),
        );
    }
    let failed: Vec<&str> = witnesses
        .iter()
        .filter(|w| w.outcome != "passed")
        .map(|w| w.test_id.as_str())
        .collect();
    if !failed.is_empty() {
        let shown: Vec<&str> = failed.iter().take(6).copied().collect();
        let more = if failed.len() > 6 {
            format!(" (+{} more)", failed.len() - 6)
        } else {
            String::new()
        };
        return (
            "REFUSED".to_string(),
            format!(
                "bundle reproduced but {}/{n} tests failed: {}{more}",
                failed.len(),
                shown.join(", ")
            ),
        );
    }
    (
        "DISCHARGED".to_string(),
        format!("suite re-ran; all {n} {label} witnesses reproduced and passed"),
    )
}

fn short(cid: &str) -> String {
    cid.chars().take(28).collect()
}

pub fn recompute_bundle_body(
    project_dir: &Path,
    code_files: &[String],
    pinned_cid: &str,
) -> Result<Vec<u8>, String> {
    let (buf, rcid, _) = build_suite_bundle(project_dir, code_files)?;
    if rcid != pinned_cid {
        return Err(format!(
            "witness package did not reproduce: recomputed {rcid}, pinned {pinned_cid}"
        ));
    }
    Ok(buf)
}

pub fn recompute_testng_bundle_body(
    project_dir: &Path,
    code_files: &[String],
    pinned_cid: &str,
) -> Result<Vec<u8>, String> {
    let (buf, rcid, _) = build_testng_suite_bundle(project_dir, code_files)?;
    if rcid != pinned_cid {
        return Err(format!(
            "witness package did not reproduce: recomputed {rcid}, pinned {pinned_cid}"
        ));
    }
    Ok(buf)
}

pub fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}
