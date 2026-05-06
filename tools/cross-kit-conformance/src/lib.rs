use provekit_canonicalizer::blake3_512_of;
use serde::Deserialize;
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, String>;

const EXPECTED_CATALOG_VERSION: &str = "v1.6.0-2026-05-05";
const EXPECTED_CATALOG_CID: &str = concat!(
    "blake3-512:",
    "ce04a40534986a95362d5f130fd3a1a667b7a157f0554f262af11ec7a2ac8e8b80",
    "f56c36cca93d7a180535eedc99949d760fce6ab63c405de8837fa20f00e781"
);

const CORE_FIXTURES: &[&str] = &[
    "eq_atomic",
    "pattern1_bounded_loop",
    "contract_decl",
    "bridge_decl_v1_1",
];

const RUST_CORE_FIXTURES: &[&str] = &["eq_atomic", "pattern1_bounded_loop", "contract_decl"];

const ALL_KITS: &[&str] = &[
    "rust",
    "go",
    "cpp",
    "typescript",
    "csharp",
    "java",
    "python",
    "c",
    "zig",
    "php",
    "swift",
    "ruby",
];

#[derive(Debug, Clone, Deserialize)]
pub struct Fixture {
    pub name: String,
    pub capability: String,
    pub description: String,
    pub jcs: String,
    pub hash: String,
}

#[derive(Debug, Deserialize)]
struct FixtureToml {
    catalog_version: String,
    catalog_cid: String,
    fixture: Vec<Fixture>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    Linux,
    Swift,
    All,
}

impl Profile {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "linux" => Ok(Self::Linux),
            "swift" => Ok(Self::Swift),
            "all" => Ok(Self::All),
            _ => Err(format!(
                "unknown profile `{s}`; expected linux, swift, or all"
            )),
        }
    }

    fn default_for_host() -> Self {
        if cfg!(target_os = "macos") {
            Self::All
        } else {
            Self::Linux
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Swift => "swift",
            Self::All => "all",
        }
    }

    fn required_kits(self) -> Vec<&'static str> {
        match self {
            Self::Linux => ALL_KITS
                .iter()
                .copied()
                .filter(|kit| *kit != "swift")
                .collect(),
            Self::Swift => vec!["swift"],
            Self::All => ALL_KITS.to_vec(),
        }
    }
}

#[derive(Debug)]
struct ProcResult {
    code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
struct DirectAdapter {
    kit: &'static str,
    emit: fn(&str) -> Result<String>,
    fixtures: &'static [&'static str],
}

#[derive(Debug)]
struct NativeCheck {
    kit: &'static str,
    name: &'static str,
    cmd: Vec<String>,
    cwd: PathBuf,
    timeout: Duration,
}

#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system time before UNIX_EPOCH: {e}"))?
            .as_nanos();
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("{prefix}_{}_{}_{}", std::process::id(), nanos, id));
        fs::create_dir_all(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tool dir has parent")
        .parent()
        .expect("tools dir has parent")
        .to_path_buf()
}

fn fixtures_toml() -> PathBuf {
    repo_root().join("conformance/fixtures.toml")
}

fn catalog_json() -> PathBuf {
    repo_root().join("protocol/specs/2026-04-30-protocol-catalog.json")
}

fn json_lit(s: &str) -> String {
    serde_json::to_string(s).expect("string JSON serialization cannot fail")
}

fn write_file(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

fn require_fixture<'a>(
    fixtures: &'a std::collections::BTreeMap<String, Fixture>,
    name: &str,
) -> Result<&'a Fixture> {
    fixtures
        .get(name)
        .ok_or_else(|| format!("missing conformance fixture `{name}`"))
}

fn run_cmd(cmd: &[String], cwd: &Path, timeout: Duration) -> ProcResult {
    run_cmd_env(cmd, cwd, timeout, &[])
}

fn run_cmd_env(
    cmd: &[String],
    cwd: &Path,
    timeout: Duration,
    envs: &[(&str, OsString)],
) -> ProcResult {
    if cmd.is_empty() {
        return ProcResult {
            code: 127,
            stdout: String::new(),
            stderr: "empty command".to_string(),
        };
    }

    let mut command = Command::new(&cmd[0]);
    command
        .args(&cmd[1..])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return ProcResult {
                code: 127,
                stdout: String::new(),
                stderr: e.to_string(),
            }
        }
    };

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().expect("completed child output");
                return ProcResult {
                    code: output.status.code().unwrap_or(1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                };
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let output = child.wait_with_output().expect("killed child output");
                    let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    stderr.push_str(&format!("\ntimeout after {}s", timeout.as_secs()));
                    return ProcResult {
                        code: -1,
                        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                        stderr,
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return ProcResult {
                    code: 1,
                    stdout: String::new(),
                    stderr: e.to_string(),
                }
            }
        }
    }
}

fn command_stdout(cmd: &[String], cwd: &Path, timeout: Duration) -> Result<String> {
    let p = run_cmd(cmd, cwd, timeout);
    if p.code != 0 {
        return Err(command_error(cmd, &p));
    }
    Ok(p.stdout.trim().to_string())
}

fn command_error(cmd: &[String], p: &ProcResult) -> String {
    let output = format!("{}\n{}", p.stderr.trim(), p.stdout.trim());
    format!(
        "{} exited {}:\n{}",
        cmd.join(" "),
        p.code,
        tail(&output, 4000)
    )
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        chars[chars.len() - n..].iter().collect()
    }
}

fn cid_is_well_formed(s: &str) -> bool {
    let Some(hex) = s.strip_prefix("blake3-512:") else {
        return false;
    };
    hex.len() == 128
        && hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn load_fixtures() -> Result<FixtureToml> {
    let raw = fs::read_to_string(fixtures_toml())
        .map_err(|e| format!("read {}: {e}", fixtures_toml().display()))?;
    toml::from_str(&raw).map_err(|e| format!("parse {}: {e}", fixtures_toml().display()))
}

fn assert_catalog_pin(f: &FixtureToml) -> Result<()> {
    if f.catalog_version != EXPECTED_CATALOG_VERSION {
        return Err(format!(
            "fixtures.toml catalog_version={:?}; expected {:?}",
            f.catalog_version, EXPECTED_CATALOG_VERSION
        ));
    }
    if f.catalog_cid != EXPECTED_CATALOG_CID {
        return Err("fixtures.toml catalog_cid does not match v1.6.0".to_string());
    }

    let catalog_text = fs::read_to_string(catalog_json())
        .map_err(|e| format!("read {}: {e}", catalog_json().display()))?;
    let catalog: serde_json::Value =
        serde_json::from_str(&catalog_text).map_err(|e| format!("parse catalog JSON: {e}"))?;
    let version = catalog
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if version != EXPECTED_CATALOG_VERSION {
        return Err(format!(
            "protocol catalog version={version:?}; expected {EXPECTED_CATALOG_VERSION:?}"
        ));
    }
    Ok(())
}

fn assert_fixture_hash_pins(f: &FixtureToml) -> Result<()> {
    for fixture in &f.fixture {
        let got = blake3_512_of(fixture.jcs.as_bytes());
        if got != fixture.hash {
            return Err(format!(
                "fixture `{}` hash pin drift:\n  got:  {}\n  want: {}",
                fixture.name, got, fixture.hash
            ));
        }
    }
    Ok(())
}

fn make_fixture_map(f: FixtureToml) -> std::collections::BTreeMap<String, Fixture> {
    f.fixture
        .into_iter()
        .map(|fixture| (fixture.name.clone(), fixture))
        .collect()
}

fn rust_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let tmp = TempDir::new("pk_rust_conformance")?;
    write_file(
        &tmp.path().join("Cargo.toml"),
        &format!(
            r#"[package]
name = "pk-rust-conformance"
version = "0.0.0"
edition = "2021"

[dependencies]
provekit-canonicalizer = {{ path = "{}" }}
provekit-ir-symbolic = {{ path = "{}" }}
"#,
            root.join("implementations/rust/provekit-canonicalizer")
                .display(),
            root.join("implementations/rust/provekit-ir-symbolic")
                .display()
        ),
    )?;
    let code = format!(
        r#"
use provekit_canonicalizer::{{blake3_512_of, encode_jcs, Value}};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{{
    and_, eq, gte, implies, lt, make_var, num, str_const, Int, Term,
}};
use std::rc::Rc;

fn parse_int_arg(arg: Rc<Term>) -> Rc<Term> {{
    Rc::new(Term::Ctor {{
        name: "parse_int".into(),
        args: vec![arg],
    }})
}}

fn main() {{
    let jcs = match {} {{
        "eq_atomic" => {{
            let f = eq(parse_int_arg(str_const("42")), num(42));
            encode_jcs(&formula_to_value(&f))
        }}
        "pattern1_bounded_loop" => {{
            let x = make_var("x");
            let body = implies(
                and_(vec![gte(x.clone(), num(0)), lt(x.clone(), num(100))]),
                gte(x, num(0)),
            );
            let q = provekit_ir_symbolic::Formula::Quantifier {{
                kind: "forall".into(),
                name: "x".into(),
                sort: Int(),
                body,
            }};
            encode_jcs(&formula_to_value(&q))
        }}
        "contract_decl" => {{
            let pre = gte(make_var("x"), num(0));
            let value = Value::array(vec![Value::object([
                ("kind", Value::string("contract")),
                ("name", Value::string("parseInt")),
                ("outBinding", Value::string("out")),
                ("pre", formula_to_value(&pre)),
            ])]);
            encode_jcs(&value)
        }}
        _ => panic!("unknown fixture"),
    }};
    print!("{{}}", blake3_512_of(jcs.as_bytes()));
}}
"#,
        json_lit(name)
    );
    write_file(&tmp.path().join("src/main.rs"), &code)?;
    command_stdout(
        &["cargo".into(), "run".into(), "--quiet".into()],
        tmp.path(),
        Duration::from_secs(180),
    )
}

fn python_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let py_src = root.join("implementations/python/provekit-lift-py-tests/src");
    let code = r#"
import sys
from provekit_lift_py_tests.canonicalizer import encode_jcs, blake3_512_of
from provekit_lift_py_tests.ir import (
    BridgeDecl, ContractDecl, Int, _Quantifier, and_, bridge_decl_to_value,
    contract_decl_to_value, ctor, declarations_to_value, eq, formula_to_value,
    gte, implies, lt, make_var, num, str_const,
)

name = sys.argv[1]
if name == "eq_atomic":
    jcs = encode_jcs(formula_to_value(eq(ctor("parse_int", [str_const("42")]), num(42))))
elif name == "pattern1_bounded_loop":
    x = make_var("x")
    body = implies(and_([gte(x, num(0)), lt(x, num(100))]), gte(x, num(0)))
    jcs = encode_jcs(formula_to_value(_Quantifier("forall", "x", Int(), body)))
elif name == "contract_decl":
    pre = gte(make_var("x"), num(0))
    jcs = encode_jcs(declarations_to_value([ContractDecl(name="parseInt", pre=pre)]))
elif name == "bridge_decl_v1_1":
    bridge = BridgeDecl(
        name="myBridge",
        source_symbol="source",
        source_layer="c-kit",
        source_contract_cid="bafySource",
        target_contract_cid="bafyTarget",
        target_proof_cid="bafyProof",
        target_layer="coq",
        notes="some notes",
    )
    jcs = encode_jcs(bridge_decl_to_value(bridge))
else:
    raise SystemExit(f"unknown fixture: {name}")
print(blake3_512_of(jcs.encode("utf-8")), end="")
"#;
    let p = run_cmd_env(
        &["python3".into(), "-c".into(), code.into(), name.into()],
        &root,
        Duration::from_secs(60),
        &[("PYTHONPATH", OsString::from(py_src))],
    );
    if p.code != 0 {
        Err(command_error(
            &["python3".into(), "-c".into(), "<python-kit-adapter>".into()],
            &p,
        ))
    } else {
        Ok(p.stdout.trim().to_string())
    }
}

fn go_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let module = root.join("implementations/go/provekit-ir-symbolic");
    let unique = format!(
        "pk_conformance_{}_{}.go",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let src = module.join(unique);
    let code = format!(
        r#"
package main

import (
    "encoding/json"
    "fmt"
    "strings"
    canon "github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
    ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

func emit(raw []byte) {{
    var v any
    dec := json.NewDecoder(strings.NewReader(string(raw)))
    dec.UseNumber()
    if err := dec.Decode(&v); err != nil {{
        panic(err)
    }}
    out, err := canon.EncodeJCS(v)
    if err != nil {{
        panic(err)
    }}
    fmt.Print(canon.ComputeCID(out))
}}

func main() {{
    switch {} {{
    case "eq_atomic":
        f := ir.Eq(ir.MakeCtor("parse_int", []ir.IrTerm{{ir.StrConst("42")}}, ir.Int), ir.Num(42))
        out, _ := json.Marshal(f)
        emit(out)
    case "pattern1_bounded_loop":
        f := ir.ForAllNamed("x", ir.Int, func(x ir.IrTerm) ir.IrFormula {{
            return ir.Implies(ir.And(ir.Gte(x, ir.Num(0)), ir.Lt(x, ir.Num(100))), ir.Gte(x, ir.Num(0)))
        }})
        out, _ := json.Marshal(f)
        emit(out)
    case "contract_decl":
        ir.ResetCollector()
        finish := ir.BeginCollecting()
        ir.Contract("parseInt", ir.ContractArgs{{Pre: ir.Gte(ir.MakeVar("x", ir.Int), ir.Num(0))}})
        decls := finish()
        out, _ := ir.MarshalDeclarations(decls)
        emit(out)
    case "bridge_decl_v1_1":
        b := ir.BridgeDeclaration{{
            Name: "myBridge",
            SourceSymbol: "source",
            SourceLayer: "c-kit",
            SourceContractCid: "bafySource",
            TargetContractCid: "bafyTarget",
            TargetProofCid: "bafyProof",
            TargetLayer: "coq",
            Notes: "some notes",
        }}
        out, _ := json.Marshal(b)
        emit(out)
    default:
        panic("unknown fixture")
    }}
}}
"#,
        json_lit(name)
    );
    write_file(&src, &code)?;
    let result = command_stdout(
        &[
            "go".into(),
            "run".into(),
            src.file_name().unwrap().to_string_lossy().into_owned(),
        ],
        &module,
        Duration::from_secs(60),
    );
    let _ = fs::remove_file(&src);
    result
}

fn c_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let c_dir = root.join("implementations/c/provekit-ir");
    let b3 = root.join("tools/blake3-vendored");
    let body = match name {
        "eq_atomic" => {
            r#"
            pk_term *arg = pk_term_const_str("42", pk_sort_primitive("String"));
            pk_term *args1[] = { arg };
            pk_term *lhs = pk_term_ctor_new("parse_int", args1, 1);
            pk_term *rhs = pk_term_const_int(42, pk_sort_primitive("Int"));
            pk_term *args2[] = { lhs, rhs };
            pk_formula *f = pk_formula_atomic_new("=", args2, 2);
            pk_emit_formula(buf, f);
            pk_formula_free(f);
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            pk_term *x1 = pk_term_var_new("x");
            pk_term *zero1 = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *lower_args[] = { x1, zero1 };
            pk_formula *lower = pk_formula_atomic_new("≥", lower_args, 2);
            pk_term *x2 = pk_term_var_new("x");
            pk_term *hundred = pk_term_const_int(100, pk_sort_primitive("Int"));
            pk_term *upper_args[] = { x2, hundred };
            pk_formula *upper = pk_formula_atomic_new("<", upper_args, 2);
            pk_formula *conj[] = { lower, upper };
            pk_formula *ant = pk_formula_connective_new("and", conj, 2);
            pk_term *x3 = pk_term_var_new("x");
            pk_term *zero2 = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *inner_args[] = { x3, zero2 };
            pk_formula *inner = pk_formula_atomic_new("≥", inner_args, 2);
            pk_formula *impl[] = { ant, inner };
            pk_formula *body = pk_formula_connective_new("implies", impl, 2);
            pk_formula *q = pk_formula_quantifier_new("forall", "x", pk_sort_primitive("Int"), body);
            pk_emit_formula(buf, q);
            pk_formula_free(q);
"#
        }
        "contract_decl" => {
            r#"
            pk_term *x = pk_term_var_new("x");
            pk_term *zero = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *args[] = { x, zero };
            pk_formula *pre = pk_formula_atomic_new("≥", args, 2);
            pk_decl *d = pk_decl_contract_new("parseInt", "out", pre, NULL, NULL);
            pk_decl *decls[] = { d };
            pk_emit_decls(buf, decls, 1);
            pk_decl_free(d);
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            pk_decl *d = pk_decl_bridge_new("myBridge", "source", "c-kit", "bafySource", "bafyTarget", "bafyProof", "coq", "some notes");
            pk_emit_decl(buf, d);
            pk_decl_free(d);
"#
        }
        _ => return Err(format!("unknown C fixture `{name}`")),
    };

    let tmp = TempDir::new("pk_c_conformance")?;
    let src = tmp.path().join("main.c");
    let out = tmp.path().join("main");
    write_file(
        &src,
        &format!(
            r#"
#include "provekit/ir.h"
#include <stdio.h>
#include <stdlib.h>
int main(void) {{
    pk_buffer *buf = pk_buffer_new();
    {body}
    char *cid = pk_hash_jcs(buf->data);
    printf("%s", cid);
    free(cid);
    pk_buffer_free(buf);
    return 0;
}}
"#
        ),
    )?;
    let mut cmd = vec![
        "cc".to_string(),
        "-std=c11".to_string(),
        "-DBLAKE3_NO_AVX2".to_string(),
        "-DBLAKE3_NO_AVX512".to_string(),
        "-DBLAKE3_NO_SSE2".to_string(),
        "-DBLAKE3_NO_SSE41".to_string(),
        "-DBLAKE3_USE_NEON=0".to_string(),
        "-I".to_string(),
        c_dir.join("include").display().to_string(),
        "-I".to_string(),
        b3.display().to_string(),
        src.display().to_string(),
    ];
    for rel in ["src/ir.c", "src/jcs.c", "src/hash.c"] {
        cmd.push(c_dir.join(rel).display().to_string());
    }
    for rel in ["blake3.c", "blake3_dispatch.c", "blake3_portable.c"] {
        cmd.push(b3.join(rel).display().to_string());
    }
    cmd.extend(["-o".to_string(), out.display().to_string()]);
    let p = run_cmd(&cmd, &root, Duration::from_secs(60));
    if p.code != 0 {
        return Err(command_error(&cmd, &p));
    }
    command_stdout(&[out.display().to_string()], &root, Duration::from_secs(20))
}

fn cpp_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let ir_include = root.join("implementations/cpp/provekit-ir-symbolic/include");
    let canon = root.join("implementations/cpp/provekit/canonicalizer");
    let b3 = root.join("tools/blake3-vendored");
    let body = match name {
        "eq_atomic" => {
            r#"
            auto lhs = std::make_shared<Term>(Term{CtorTerm{"parse_int", {str_const("42")}}});
            auto rhs = num(42);
            auto f = std::make_shared<Formula>(Formula{AtomicFormula{"=", {lhs, rhs}}});
            write_formula(out, *f);
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            auto x1 = make_var("x");
            auto x2 = make_var("x");
            auto x3 = make_var("x");
            auto lower = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x1, num(0)}}});
            auto upper = std::make_shared<Formula>(Formula{AtomicFormula{"<", {x2, num(100)}}});
            auto ant = std::make_shared<Formula>(Formula{ConnectiveFormula{"and", {lower, upper}}});
            auto inner = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x3, num(0)}}});
            auto body = std::make_shared<Formula>(Formula{ConnectiveFormula{"implies", {ant, inner}}});
            auto q = std::make_shared<Formula>(Formula{QuantifierFormula{"forall", "x", Int(), body}});
            write_formula(out, *q);
"#
        }
        "contract_decl" => {
            r#"
            auto pre = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {make_var("x"), num(0)}}});
            std::vector<ContractDecl> decls{ContractDecl{"parseInt", pre, nullptr, nullptr, "out", nullptr}};
            out << marshal_declarations(decls);
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            BridgeDecl b;
            b.name = "myBridge";
            b.source_symbol = "source";
            b.source_layer = "c-kit";
            b.source_contract_cid = "bafySource";
            b.target_contract_cid = "bafyTarget";
            b.target_proof_cid = "bafyProof";
            b.target_layer = "coq";
            b.notes = "some notes";
            write_bridge_decl(out, b);
"#
        }
        _ => return Err(format!("unknown C++ fixture `{name}`")),
    };
    let tmp = TempDir::new("pk_cpp_conformance")?;
    let src = tmp.path().join("main.cpp");
    let out_bin = tmp.path().join("main");
    write_file(
        &src,
        &format!(
            r#"
#include "provekit/ir.hpp"
#include "hash.hpp"
#include <iostream>
#include <sstream>
using namespace provekit::ir;
int main() {{
    std::ostringstream out;
    {body}
    std::cout << provekit::canonicalizer::compute_cid(out.str());
    return 0;
}}
"#
        ),
    )?;

    let b3_flags = [
        "-DBLAKE3_NO_AVX2",
        "-DBLAKE3_NO_AVX512",
        "-DBLAKE3_NO_SSE2",
        "-DBLAKE3_NO_SSE41",
        "-DBLAKE3_USE_NEON=0",
    ];
    let mut objects = Vec::new();
    for rel in ["blake3.c", "blake3_dispatch.c", "blake3_portable.c"] {
        let obj = tmp.path().join(format!("{rel}.o").replace('/', "_"));
        let mut cmd = vec!["cc".to_string(), "-std=c11".to_string()];
        cmd.extend(b3_flags.iter().map(|s| s.to_string()));
        cmd.extend([
            "-I".to_string(),
            b3.display().to_string(),
            "-c".to_string(),
            b3.join(rel).display().to_string(),
            "-o".to_string(),
            obj.display().to_string(),
        ]);
        let p = run_cmd(&cmd, &root, Duration::from_secs(30));
        if p.code != 0 {
            return Err(command_error(&cmd, &p));
        }
        objects.push(obj);
    }
    let hash_obj = tmp.path().join("hash.o");
    let hash_cmd = vec![
        "c++".to_string(),
        "-std=c++17".to_string(),
        "-I".to_string(),
        canon.display().to_string(),
        "-I".to_string(),
        b3.display().to_string(),
        "-c".to_string(),
        canon.join("hash.cpp").display().to_string(),
        "-o".to_string(),
        hash_obj.display().to_string(),
    ];
    let p = run_cmd(&hash_cmd, &root, Duration::from_secs(30));
    if p.code != 0 {
        return Err(command_error(&hash_cmd, &p));
    }
    let mut link = vec![
        "c++".to_string(),
        "-std=c++17".to_string(),
        "-I".to_string(),
        ir_include.display().to_string(),
        "-I".to_string(),
        canon.display().to_string(),
        "-I".to_string(),
        b3.display().to_string(),
        src.display().to_string(),
        hash_obj.display().to_string(),
    ];
    link.extend(objects.iter().map(|p| p.display().to_string()));
    link.extend(["-o".to_string(), out_bin.display().to_string()]);
    let p = run_cmd(&link, &root, Duration::from_secs(60));
    if p.code != 0 {
        return Err(command_error(&link, &p));
    }
    command_stdout(
        &[out_bin.display().to_string()],
        &root,
        Duration::from_secs(20),
    )
}

fn zig_tool() -> Result<PathBuf> {
    let bundled = repo_root().join("zig-toolchain/zig");
    if bundled.exists() {
        return Ok(bundled);
    }
    let p = run_cmd(
        &["sh".into(), "-c".into(), "command -v zig".into()],
        &repo_root(),
        Duration::from_secs(5),
    );
    if p.code == 0 && !p.stdout.trim().is_empty() {
        Ok(PathBuf::from(p.stdout.trim()))
    } else {
        Err("required tool not found: zig".to_string())
    }
}

fn zig_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let zig = zig_tool()?;
    let src_dir = root.join("implementations/zig/provekit-ir/src");
    let body = match name {
        "eq_atomic" => {
            r#"
            const ctor_args = [_]provekit.Term{provekit.Str("42")};
            const lhs = provekit.Ctor("parse_int", &ctor_args);
            const rhs = provekit.Num(42);
            const atomic_args = [_]provekit.Term{ lhs, rhs };
            const value = provekit.Atomic("=", &atomic_args);
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            const lower_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const lower = provekit.Atomic("≥", &lower_args);
            const upper_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(100) };
            const upper = provekit.Atomic("<", &upper_args);
            const conj_args = [_]provekit.Formula{ lower, upper };
            const ant = provekit.And(&conj_args);
            const inner_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const inner = provekit.Atomic("≥", &inner_args);
            const impl_args = [_]provekit.Formula{ ant, inner };
            const body = provekit.Implies(&impl_args);
            const value = provekit.Forall("x", provekit.Sort.Int, &body);
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
"#
        }
        "contract_decl" => {
            r#"
            const pre_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const pre = provekit.Atomic("≥", &pre_args);
            const decl = provekit.Decl{ .contract = .{ .name = "parseInt", .out_binding = "out", .pre = pre } };
            const decls = [_]provekit.Decl{decl};
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, &decls);
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            const value = provekit.Decl{ .bridge = .{
                .name = "myBridge",
                .source_symbol = "source",
                .source_layer = "c-kit",
                .source_contract_cid = "bafySource",
                .target_contract_cid = "bafyTarget",
                .target_proof_cid = "bafyProof",
                .target_layer = "coq",
                .notes = "some notes",
            } };
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
"#
        }
        _ => return Err(format!("unknown Zig fixture `{name}`")),
    };
    let tmp = TempDir::new("pk_zig_conformance")?;
    fs::copy(src_dir.join("root.zig"), tmp.path().join("root.zig"))
        .map_err(|e| format!("copy root.zig: {e}"))?;
    fs::copy(
        src_dir.join("cross_kit_bridges.zig"),
        tmp.path().join("cross_kit_bridges.zig"),
    )
    .map_err(|e| format!("copy cross_kit_bridges.zig: {e}"))?;
    write_file(
        &tmp.path().join("main.zig"),
        &format!(
            r#"
const std = @import("std");
const provekit = @import("provekit-ir");
pub fn main(init: std.process.Init) !void {{
    {body}
    defer std.heap.page_allocator.free(jcs);
    const cid = try provekit.jcsHash(std.heap.page_allocator, jcs);
    defer std.heap.page_allocator.free(cid);
    var write_buf: [4096]u8 = undefined;
    var stdout_file = std.Io.File.stdout().writerStreaming(init.io, &write_buf);
    var stdout_writer = &stdout_file.interface;
    try stdout_writer.print("{{s}}", .{{cid}});
    try stdout_writer.flush();
}}
"#
        ),
    )?;
    write_file(
        &tmp.path().join("build.zig"),
        r#"
const std = @import("std");
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const provekit_ir = b.createModule(.{
        .root_source_file = b.path("root.zig"),
        .target = target,
        .optimize = optimize,
    });
    const exe_mod = b.createModule(.{
        .root_source_file = b.path("main.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "provekit-ir", .module = provekit_ir },
        },
    });
    const exe = b.addExecutable(.{
        .name = "main",
        .root_module = exe_mod,
    });
    b.installArtifact(exe);
}
"#,
    )?;
    let p = run_cmd(
        &[
            zig.display().to_string(),
            "build".into(),
            "--prefix".into(),
            ".".into(),
        ],
        tmp.path(),
        Duration::from_secs(120),
    );
    if p.code != 0 {
        return Err(command_error(
            &[
                zig.display().to_string(),
                "build".into(),
                "--prefix".into(),
                ".".into(),
            ],
            &p,
        ));
    }
    command_stdout(
        &[tmp.path().join("bin/main").display().to_string()],
        tmp.path(),
        Duration::from_secs(20),
    )
}

fn csharp_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let csharp = root.join("implementations/csharp");
    let body = match name {
        "eq_atomic" => {
            r#"
            var lhs = Terms.Ctor("parse_int", [Terms.StrConst("42")]);
            var rhs = Terms.Num(42);
            var jcs = Jcs.Encode(Serialize.FormulaToValue(Predicates.Eq(lhs, rhs)));
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            var x = Terms.Var("x");
            var lower = Predicates.Gte(x, Terms.Num(0));
            var upper = Predicates.Lt(x, Terms.Num(100));
            var ant = Predicates.And(lower, upper);
            var inner = Predicates.Gte(x, Terms.Num(0));
            var q = new QuantifierFormula("forall", "x", Sort.Int, Predicates.Implies(ant, inner));
            var jcs = Jcs.Encode(Serialize.FormulaToValue(q));
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        "contract_decl" => {
            r#"
            var pre = Predicates.Gte(Terms.Var("x"), Terms.Num(0));
            var value = Value.Array(Value.Object(
                ("kind", Value.String("contract")),
                ("name", Value.String("parseInt")),
                ("outBinding", Value.String("out")),
                ("pre", Serialize.FormulaToValue(pre))
            ));
            var jcs = Jcs.Encode(value);
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            var bridge = new BridgeDeclaration("myBridge", "source", "c-kit", "bafySource", "bafyTarget", "bafyProof", "coq", "some notes");
            var jcs = Jcs.Encode(Serialize.BridgeDeclarationToValue(bridge));
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        _ => return Err(format!("unknown C# fixture `{name}`")),
    };
    let tmp = TempDir::new("pk_cs_conformance")?;
    write_file(
        &tmp.path().join("pk_cs_conformance.csproj"),
        &format!(
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="{}" />
    <ProjectReference Include="{}" />
  </ItemGroup>
</Project>
"#,
            csharp.join("Provekit.IR/Provekit.IR.csproj").display(),
            csharp
                .join("Provekit.Canonicalizer/Provekit.Canonicalizer.csproj")
                .display()
        ),
    )?;
    write_file(
        &tmp.path().join("Program.cs"),
        &format!(
            r#"
using Provekit.Canonicalizer;
using Provekit.IR;

{body}
"#
        ),
    )?;
    command_stdout(
        &[
            "dotnet".into(),
            "run".into(),
            "--project".into(),
            "pk_cs_conformance.csproj".into(),
        ],
        tmp.path(),
        Duration::from_secs(120),
    )
}

fn ruby_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let code = r#"
require "provekit"

def emit(jcs)
  print Provekit::Blake3.hex(jcs)
end

case ARGV.fetch(0)
when "eq_atomic"
  lhs = Provekit::IR.ctor("parse_int", Provekit::IR.str("42"))
  rhs = Provekit::IR.num(42)
  emit Provekit::IR::Jcs.encode(Provekit::IR.eq(lhs, rhs))
when "pattern1_bounded_loop"
  x = Provekit::IR.var(name: "x")
  body = Provekit::IR.implies(
    Provekit::IR.and(
      Provekit::IR.gte(x, Provekit::IR.num(0)),
      Provekit::IR.lt(x, Provekit::IR.num(100)),
    ),
    Provekit::IR.gte(x, Provekit::IR.num(0)),
  )
  q = Provekit::IR.forall(name: "x", sort: Provekit::IR::PrimitiveSort.Int, body: body)
  emit Provekit::IR::Jcs.encode(q)
when "contract_decl"
  pre = Provekit::IR.gte(Provekit::IR.var(name: "x"), Provekit::IR.num(0))
  d = Provekit::IR::ContractDecl.new(name: "parseInt", pre: pre)
  emit Provekit::IR.marshal_declarations([d])
when "bridge_decl_v1_1"
  d = Provekit::IR::Bridge.new(
    name: "myBridge",
    source_symbol: "source",
    source_layer: "c-kit",
    source_contract_cid: "bafySource",
    target_contract_cid: "bafyTarget",
    target_proof_cid: "bafyProof",
    target_layer: "coq",
    notes: "some notes",
  )
  emit Provekit::IR.marshal_declarations([d])[1...-1]
else
  abort "unknown fixture"
end
"#;
    command_stdout(
        &[
            "ruby".into(),
            "-Ilib".into(),
            "-e".into(),
            code.into(),
            name.into(),
        ],
        &root.join("implementations/ruby"),
        Duration::from_secs(60),
    )
}

fn php_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let code = r#"
require "provekit-ir-symbolic/src/Canonicalizer/Jcs.php";
require "provekit-ir-symbolic/src/Canonicalizer/Blake3.php";
require "provekit-ir-symbolic/src/Ir/Term.php";
require "provekit-ir-symbolic/src/Ir/Formula.php";
require "provekit-ir-symbolic/src/Ir/Declaration.php";

function emit($value) {
    echo \ProvekIt\Canonicalizer\Blake3::cid(\ProvekIt\Canonicalizer\Jcs::encode($value));
}

$name = $argv[1] ?? "";
switch ($name) {
case "eq_atomic":
    emit(\ProvekIt\Ir\Eq(
        \ProvekIt\Ir\Ctor("parse_int", \ProvekIt\Ir\Str("42")),
        \ProvekIt\Ir\Num(42)
    ));
    break;
case "pattern1_bounded_loop":
    $x = \ProvekIt\Ir\V("x");
    $body = \ProvekIt\Ir\Implies(
        \ProvekIt\Ir\And_(
            \ProvekIt\Ir\Gte($x, \ProvekIt\Ir\Num(0)),
            \ProvekIt\Ir\Lt($x, \ProvekIt\Ir\Num(100))
        ),
        \ProvekIt\Ir\Gte($x, \ProvekIt\Ir\Num(0))
    );
    emit(\ProvekIt\Ir\ForAll("x", \ProvekIt\Ir\Sort::Int(), $body));
    break;
case "contract_decl":
    $pre = \ProvekIt\Ir\Gte(\ProvekIt\Ir\V("x"), \ProvekIt\Ir\Num(0));
    emit([new \ProvekIt\Ir\ContractDecl("parseInt", "out", $pre)]);
    break;
case "bridge_decl_v1_1":
    emit(new \ProvekIt\Ir\BridgeDecl(
        "myBridge",
        "source",
        "c-kit",
        "bafySource",
        "bafyTarget",
        "bafyProof",
        "coq",
        "some notes"
    ));
    break;
default:
    fwrite(STDERR, "unknown fixture\n");
    exit(1);
}
"#;
    command_stdout(
        &["php".into(), "-r".into(), code.into(), name.into()],
        &root.join("implementations/php"),
        Duration::from_secs(60),
    )
}

fn java_classpath() -> Result<String> {
    let root = repo_root();
    let java_root = root.join("implementations/java");
    let package_cmd = vec![
        "mvn".to_string(),
        "-q".to_string(),
        "-f".to_string(),
        "implementations/java/pom.xml".to_string(),
        "-pl".to_string(),
        "provekit-ir,provekit-claim-envelope".to_string(),
        "-am".to_string(),
        "package".to_string(),
        "-DskipTests".to_string(),
    ];
    let p = run_cmd(&package_cmd, &root, Duration::from_secs(180));
    if p.code != 0 {
        return Err(command_error(&package_cmd, &p));
    }
    let cp_file = java_root.join("provekit-claim-envelope/target/classpath.txt");
    let dep_cmd = vec![
        "mvn".to_string(),
        "-q".to_string(),
        "-f".to_string(),
        "implementations/java/provekit-claim-envelope/pom.xml".to_string(),
        "dependency:build-classpath".to_string(),
        format!("-Dmdep.outputFile={}", cp_file.display()),
    ];
    let p = run_cmd(&dep_cmd, &root, Duration::from_secs(120));
    if p.code != 0 {
        return Err(command_error(&dep_cmd, &p));
    }
    let mut parts = vec![
        java_root
            .join("provekit-ir/target/classes")
            .display()
            .to_string(),
        java_root
            .join("provekit-claim-envelope/target/classes")
            .display()
            .to_string(),
    ];
    if let Ok(extra) = fs::read_to_string(cp_file) {
        let trimmed = extra.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    Ok(parts.join(if cfg!(windows) { ";" } else { ":" }))
}

fn java_emit_cid(name: &str) -> Result<String> {
    let cp = java_classpath()?;
    let tmp = TempDir::new("pk_java_conformance")?;
    let code = format!(
        r#"
import com.provekit.ir.*;
import com.provekit.claimenvelope.Blake3;
import java.nio.charset.StandardCharsets;

public class PkJavaConformance {{
  private static void emit(String jcs) {{
    System.out.print(Blake3.blake3_512(jcs.getBytes(StandardCharsets.UTF_8)));
  }}

  public static void main(String[] args) {{
    switch ({}) {{
      case "eq_atomic" -> {{
        Term lhs = Term.ctor("parse_int", new Term[]{{ Term.const_("42", Sort.String) }}, Sort.Int);
        Term rhs = Term.const_(42, Sort.Int);
        emit(Formula.atomic("=", lhs, rhs).toJson());
      }}
      case "pattern1_bounded_loop" -> {{
        Term x = Term.var_("x", Sort.Int);
        Formula lower = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
        Formula upper = Formula.atomic("<", x, Term.const_(100, Sort.Int));
        Formula ant = Formula.and(lower, upper);
        Formula inner = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
        emit(Formula.forall("x", Sort.Int, Formula.implies(ant, inner)).toJson());
      }}
      case "contract_decl" -> {{
        Term x = Term.var_("x", Sort.Int);
        Formula pre = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
        Declaration.Contract d = new Declaration.Contract("parseInt", "out", pre, null, null, null);
        emit("[" + d.toJson() + "]");
      }}
      case "bridge_decl_v1_1" -> {{
        Declaration.Bridge b = new Declaration.Bridge(
          "myBridge", "source", "c-kit", "bafySource", "bafyTarget",
          "bafyProof", "coq", "some notes");
        emit(b.toJson());
      }}
      default -> throw new IllegalArgumentException("unknown fixture");
    }}
  }}
}}
"#,
        json_lit(name)
    );
    let src = tmp.path().join("PkJavaConformance.java");
    write_file(&src, &code)?;
    let p = run_cmd(
        &[
            "javac".into(),
            "-cp".into(),
            cp.clone(),
            src.display().to_string(),
        ],
        tmp.path(),
        Duration::from_secs(60),
    );
    if p.code != 0 {
        return Err(command_error(
            &[
                "javac".into(),
                "-cp".into(),
                cp.clone(),
                src.display().to_string(),
            ],
            &p,
        ));
    }
    command_stdout(
        &[
            "java".into(),
            "-cp".into(),
            format!(
                "{cp}{}{}",
                if cfg!(windows) { ";" } else { ":" },
                tmp.path().display()
            ),
            "PkJavaConformance".into(),
        ],
        tmp.path(),
        Duration::from_secs(60),
    )
}

fn swift_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    command_stdout(
        &[
            "swift".into(),
            "run".into(),
            "conformance".into(),
            "--fixture".into(),
            name.into(),
        ],
        &root.join("implementations/swift"),
        Duration::from_secs(180),
    )
}

fn linux_direct_adapters() -> Vec<DirectAdapter> {
    vec![
        DirectAdapter {
            kit: "rust",
            emit: rust_emit_cid,
            fixtures: RUST_CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "python",
            emit: python_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "go",
            emit: go_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "c",
            emit: c_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "cpp",
            emit: cpp_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "zig",
            emit: zig_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "csharp",
            emit: csharp_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "ruby",
            emit: ruby_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "php",
            emit: php_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "java",
            emit: java_emit_cid,
            fixtures: CORE_FIXTURES,
        },
    ]
}

fn swift_direct_adapters() -> Vec<DirectAdapter> {
    vec![DirectAdapter {
        kit: "swift",
        emit: swift_emit_cid,
        fixtures: CORE_FIXTURES,
    }]
}

fn linux_native_checks() -> Vec<NativeCheck> {
    let root = repo_root();
    vec![
        NativeCheck {
            kit: "rust",
            name: "rust bridge_v1_4 fixture CID",
            cmd: vec![
                "cargo".into(),
                "test".into(),
                "--release".into(),
                "--manifest-path".into(),
                "implementations/rust/Cargo.toml".into(),
                "-p".into(),
                "provekit-claim-envelope".into(),
                "--test".into(),
                "bridge_v14_roundtrip".into(),
            ],
            cwd: root.clone(),
            timeout: Duration::from_secs(300),
        },
        NativeCheck {
            kit: "typescript",
            name: "typescript fixture CIDs",
            cmd: vec![
                "pnpm".into(),
                "exec".into(),
                "vitest".into(),
                "run".into(),
                "implementations/typescript/src/canonicalizer/cross-impl-golden.test.ts".into(),
            ],
            cwd: root.clone(),
            timeout: Duration::from_secs(180),
        },
        NativeCheck {
            kit: "ruby",
            name: "ruby bridge_v1_4 fixture CID",
            cmd: vec![
                "ruby".into(),
                "-Ilib".into(),
                "-Itest".into(),
                "test/test_bridge_v14.rb".into(),
            ],
            cwd: root.join("implementations/ruby"),
            timeout: Duration::from_secs(120),
        },
        NativeCheck {
            kit: "java",
            name: "java bridge_v1_4 fixture CID",
            cmd: vec![
                "mvn".into(),
                "test".into(),
                "-q".into(),
                "-f".into(),
                "implementations/java/provekit-claim-envelope/pom.xml".into(),
                "-Dtest=BridgeV14RoundtripTest".into(),
            ],
            cwd: root.clone(),
            timeout: Duration::from_secs(180),
        },
        NativeCheck {
            kit: "csharp",
            name: "csharp bridge_v1_4 fixture CID",
            cmd: vec![
                "dotnet".into(),
                "test".into(),
                "implementations/csharp/Provekit.Tests/Provekit.Tests.csproj".into(),
                "--filter".into(),
                "BridgeV14".into(),
                "--nologo".into(),
                "--verbosity".into(),
                "quiet".into(),
            ],
            cwd: root,
            timeout: Duration::from_secs(180),
        },
    ]
}

fn swift_native_checks() -> Vec<NativeCheck> {
    let root = repo_root();
    vec![NativeCheck {
        kit: "swift",
        name: "swift conformance runner CID checks",
        cmd: vec!["swift".into(), "run".into(), "conformance".into()],
        cwd: root.join("implementations/swift"),
        timeout: Duration::from_secs(300),
    }]
}

fn assert_profile_inventory(
    profile: Profile,
    direct: &[DirectAdapter],
    native: &[NativeCheck],
) -> Result<()> {
    let required: HashSet<&str> = profile.required_kits().into_iter().collect();
    let covered: HashSet<&str> = direct
        .iter()
        .map(|a| a.kit)
        .chain(native.iter().map(|c| c.kit))
        .collect();
    let mut missing: Vec<_> = required.difference(&covered).copied().collect();
    let mut extra: Vec<_> = covered.difference(&required).copied().collect();
    missing.sort_unstable();
    extra.sort_unstable();
    if !missing.is_empty() {
        return Err(format!(
            "{} profile leaves kit(s) uncovered: {}",
            profile.name(),
            missing.join(", ")
        ));
    }
    if !extra.is_empty() {
        return Err(format!(
            "{} profile covers unexpected kit(s): {}",
            profile.name(),
            extra.join(", ")
        ));
    }
    println!("  kits: {}", profile.required_kits().join(", "));
    Ok(())
}

fn run_direct_adapters(
    adapters: &[DirectAdapter],
    fixtures: &std::collections::BTreeMap<String, Fixture>,
) -> usize {
    let mut failures = 0;
    for adapter in adapters {
        println!("\n[{}] direct CID adapter", adapter.kit);
        for fixture_name in adapter.fixtures {
            let fixture = match require_fixture(fixtures, fixture_name) {
                Ok(fixture) => fixture,
                Err(e) => {
                    failures += 1;
                    println!("  FAIL {fixture_name}: {e}");
                    continue;
                }
            };
            let got = match (adapter.emit)(fixture_name) {
                Ok(cid) => cid,
                Err(e) => {
                    failures += 1;
                    println!("  FAIL {fixture_name}: {e}");
                    continue;
                }
            };
            if !cid_is_well_formed(&got) {
                failures += 1;
                println!("  FAIL {fixture_name}: adapter emitted malformed CID: {got:?}");
                continue;
            }
            if got == fixture.hash {
                println!("  PASS {fixture_name} ({})", fixture.capability);
            } else {
                failures += 1;
                println!("  FAIL {fixture_name}: CID mismatch");
                println!("    got:  {got}");
                println!("    want: {}", fixture.hash);
            }
        }
    }
    failures
}

fn run_native_checks(checks: &[NativeCheck]) -> usize {
    let mut failures = 0;
    for check in checks {
        println!("\n[native] {}", check.name);
        let p = run_cmd(&check.cmd, &check.cwd, check.timeout);
        if p.code == 0 {
            println!("  PASS {}", check.cmd.join(" "));
        } else {
            failures += 1;
            println!("  FAIL {}", check.cmd.join(" "));
            println!("{}", tail(&format!("{}\n{}", p.stderr, p.stdout), 4000));
        }
    }
    failures
}

fn print_help() {
    println!(
        "cross-kit-conformance\n\
         \n\
         Usage: cross-kit-conformance [--profile linux|swift|all]\n\
         \n\
         The Rust harness validates catalog-pinned fixture CIDs. Adapters may\n\
         produce any representation internally; the conformance boundary is\n\
         the protocol CID."
    );
}

fn parse_profile<I, S>(args: I) -> Result<Profile>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut profile = Profile::default_for_host();
    let mut args = args.into_iter().map(Into::into).skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--profile requires linux, swift, or all".to_string())?;
                profile = Profile::parse(&value)?;
            }
            "-h" | "--help" => {
                print_help();
                return Err("__help__".to_string());
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(profile)
}

fn run(profile: Profile) -> Result<usize> {
    let fixture_file = load_fixtures()?;
    println!("\nCatalog-pinned Cross-Kit Conformance");
    assert_catalog_pin(&fixture_file)?;
    assert_fixture_hash_pins(&fixture_file)?;
    println!(
        "  catalog: {} {}",
        fixture_file.catalog_version, fixture_file.catalog_cid
    );

    let fixtures = make_fixture_map(fixture_file);
    for name in CORE_FIXTURES.iter().chain(["bridge_decl_v1_4"].iter()) {
        require_fixture(&fixtures, name)?;
    }

    let mut direct = Vec::new();
    let mut native = Vec::new();
    if matches!(profile, Profile::Linux | Profile::All) {
        direct.extend(linux_direct_adapters());
        native.extend(linux_native_checks());
    }
    if matches!(profile, Profile::Swift | Profile::All) {
        direct.extend(swift_direct_adapters());
        native.extend(swift_native_checks());
    }
    assert_profile_inventory(profile, &direct, &native)?;

    let failures = run_direct_adapters(&direct, &fixtures) + run_native_checks(&native);
    println!("\nResult");
    if failures == 0 {
        println!("  all selected conformance CID checks passed");
    } else {
        println!("  {failures} conformance failure(s)");
    }
    Ok(failures)
}

pub fn main_entry<I, S>(args: I) -> ExitCode
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let profile = match parse_profile(args) {
        Ok(profile) => profile,
        Err(e) if e == "__help__" => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fatal: {e}");
            return ExitCode::FAILURE;
        }
    };

    match run(profile) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("fatal: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_gate_wiring_uses_rust_harness() {
        let root = repo_root();
        let makefile = fs::read_to_string(root.join("Makefile")).expect("read Makefile");
        let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI");

        assert!(
            !makefile.contains("conformance/run.py"),
            "Makefile still uses the Python conformance harness"
        );
        assert!(
            !ci.contains("conformance/run.py"),
            "CI still uses the Python conformance harness"
        );
        assert!(
            makefile.contains("tools/cross-kit-conformance/Cargo.toml"),
            "Makefile must call the Rust conformance harness"
        );
        assert!(
            ci.contains("tools/cross-kit-conformance/Cargo.toml"),
            "CI must call the Rust conformance harness"
        );
    }

    #[test]
    fn fixture_pins_are_cids_not_byte_assertions() {
        let fixture_file = load_fixtures().expect("load fixtures");
        assert_fixture_hash_pins(&fixture_file).expect("fixture JCS hashes match CID pins");
        for fixture in fixture_file.fixture {
            assert!(
                cid_is_well_formed(&fixture.hash),
                "{} hash is malformed",
                fixture.name
            );
        }
    }

    #[test]
    fn profile_inventory_covers_expected_kits() {
        let mut direct = linux_direct_adapters();
        let native = linux_native_checks();
        assert_profile_inventory(Profile::Linux, &direct, &native).expect("linux inventory");
        direct.extend(swift_direct_adapters());
        let mut native_all = native;
        native_all.extend(swift_native_checks());
        assert_profile_inventory(Profile::All, &direct, &native_all).expect("all inventory");
    }

    #[test]
    fn malformed_cids_are_rejected() {
        assert!(cid_is_well_formed(
            "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa"
        ));
        assert!(!cid_is_well_formed("blake3-512:ABC"));
        assert!(!cid_is_well_formed("sha256:abc"));
    }
}
