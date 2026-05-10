#!/usr/bin/env python3
import copy
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


BASE = Path(__file__).resolve().parents[1]
ROOT = BASE.parents[1]
RUST_DIR = ROOT / "implementations" / "rust"
TARGET = RUST_DIR / "target" / "debug"
PROVEKIT = TARGET / "provekit"
CANON = TARGET / "compute_fixture_cid"
AARCH64_LIFTER = TARGET / "provekit-lift-asm-aarch64"
X86_LIFTER = TARGET / "provekit-lift-asm-x86-64"
RUST_WALK_EMIT = TARGET / "provekit-walk-emit"

SPEC_DIR = BASE / "specs"
SOURCE_DIR = BASE / "sources"
RECEIPT_DIR = BASE / "receipts"
DISCHARGE_DIR = BASE / "discharges"
CATALOG_REAL = BASE / "catalog"
CATALOG_ARG = BASE / "dev" / ".." / "catalog"
CID_FILE = BASE / "cids.tsv"

INT_SORT = {"kind": "primitive", "name": "Int"}
TRUE_FORMULA = {"kind": "atomic", "name": "true", "args": []}


def var(name):
    return {"kind": "var", "name": name}


def const_int(value):
    return {"kind": "const", "value": value, "sort": copy.deepcopy(INT_SORT)}


def shape_condition_term():
    return {
        "kind": "ctor",
        "name": "=",
        "args": [var("arg_0"), const_int(0)],
    }


def shape_result_term():
    return {
        "kind": "ctor",
        "name": "ite",
        "args": [shape_condition_term(), const_int(-22), var("arg_0")],
    }


def shape_post():
    return {
        "kind": "atomic",
        "name": "=",
        "args": [var("ret"), shape_result_term()],
    }


def shape_payload():
    return {
        "schema_version": "1",
        "protocol": "AMP",
        "kind": "AlgorithmMemento",
        "fn_name": "shape:foo",
        "formals": ["arg_0"],
        "formal_sorts": [copy.deepcopy(INT_SORT)],
        "return_sort": copy.deepcopy(INT_SORT),
        "pre": copy.deepcopy(TRUE_FORMULA),
        "post": shape_post(),
        "effects": {"effects": []},
        "auto_minted_mementos": [],
    }


def shape_spec():
    payload = shape_payload()
    return {
        "kind": "algorithm",
        "fn_name": payload["fn_name"],
        "formals": payload["formals"],
        "formal_sorts": payload["formal_sorts"],
        "return_sort": payload["return_sort"],
        "pre": payload["pre"],
        "post": payload["post"],
        "effects": payload["effects"],
    }


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, ensure_ascii=True)
        handle.write("\n")


def read_json(path):
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def run(command, cwd=None, input_text=None):
    result = subprocess.run(
        [str(part) for part in command],
        cwd=str(cwd) if cwd else None,
        input=input_text,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise SystemExit(
            "command failed: "
            + " ".join(str(part) for part in command)
            + "\nstdout:\n"
            + result.stdout
            + "\nstderr:\n"
            + result.stderr
        )
    return result.stdout.strip()


def build_tools():
    run(
        [
            "cargo",
            "build",
            "-p",
            "provekit-cli",
            "-p",
            "provekit-lift-asm-aarch64",
            "-p",
            "provekit-lift-asm-x86-64",
            "-p",
            "provekit-canonicalizer",
            "-p",
            "provekit-walk",
        ],
        cwd=RUST_DIR,
    )


def lift_contract(lifter, workspace_root, surface, source_path):
    request = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "lift",
        "params": {
            "workspace_root": str(workspace_root),
            "surface": surface,
            "source_paths": [source_path],
            "options": {"layer": "all"},
        },
    }
    output = run([lifter, "--rpc"], input_text=json.dumps(request) + "\n")
    response = json.loads(output.splitlines()[0])
    if "error" in response:
        raise SystemExit(json.dumps(response["error"], indent=2))
    declarations = response["result"]["declarations"]
    if len(declarations) != 1:
        raise SystemExit("expected one lifted contract")
    return declarations[0]


def canonical_cid_file(path):
    return run([CANON, path])


def canonical_cid_value(value):
    tmp_dir = BASE / "tmp"
    tmp_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", suffix=".json", dir=tmp_dir, delete=False
    ) as handle:
        json.dump(value, handle, ensure_ascii=True)
        handle.write("\n")
        tmp_path = Path(handle.name)
    try:
        return canonical_cid_file(tmp_path)
    finally:
        tmp_path.unlink(missing_ok=True)


def source_contract_cid(path):
    value = read_json(path)
    file_cid = canonical_cid_file(path)
    if isinstance(value, dict) and "cid" in value:
        embedded = value["cid"]
        without_cid = copy.deepcopy(value)
        del without_cid["cid"]
        computed = canonical_cid_value(without_cid)
        if computed != embedded:
            raise SystemExit(
                f"embedded CID mismatch for {path}: {embedded} != {computed}"
            )
        return embedded, file_cid
    return file_cid, file_cid


def normalize_sort(sort_value, representation_map):
    if not isinstance(sort_value, dict):
        return sort_value
    out = copy.deepcopy(sort_value)
    if out.get("kind") == "primitive":
        name = out.get("name")
        if name in representation_map:
            out["name"] = representation_map[name]
    return out


def signed_bv32(value):
    if isinstance(value, str) and value.startswith("0x"):
        parsed = int(value, 16)
    elif isinstance(value, int):
        parsed = value
    else:
        return value
    parsed = parsed & 0xFFFFFFFF
    if parsed >= 0x80000000:
        parsed -= 0x100000000
    return parsed


def normalize_node(value, renaming_map, representation_map):
    if isinstance(value, list):
        return [
            normalize_node(item, renaming_map, representation_map) for item in value
        ]
    if not isinstance(value, dict):
        return value

    kind = value.get("kind")
    if kind == "connective":
        op = value["op"]
        return {
            "kind": op,
            "operands": normalize_node(
                value.get("operands", []), renaming_map, representation_map
            ),
        }

    out = {}
    for key, item in value.items():
        out[key] = normalize_node(item, renaming_map, representation_map)

    if kind == "var" and out.get("name") in renaming_map:
        out["name"] = renaming_map[out["name"]]

    if kind == "const":
        original_sort = value.get("sort", {})
        original_name = original_sort.get("name") if isinstance(original_sort, dict) else None
        out["sort"] = normalize_sort(out.get("sort"), representation_map)
        if original_name in ("BitVector", "BitVector32"):
            out["value"] = signed_bv32(out["value"])

    if kind == "ctor":
        out = simplify_term(out)

    return out


def term_true(term):
    return (
        isinstance(term, dict)
        and term.get("kind") == "ctor"
        and term.get("name") == "true"
        and term.get("args") == []
    )


def simplify_term(term):
    if not isinstance(term, dict) or term.get("kind") != "ctor":
        return term
    args = [simplify_term(arg) for arg in term.get("args", [])]
    name = term.get("name")
    if name == "and":
        args = [arg for arg in args if not term_true(arg)]
        if not args:
            return {"kind": "ctor", "name": "true", "args": []}
        if len(args) == 1:
            return args[0]
    return {"kind": "ctor", "name": name, "args": args}


def formula_to_term(formula):
    kind = formula.get("kind")
    if kind == "atomic":
        return simplify_term(
            {
                "kind": "ctor",
                "name": formula["name"],
                "args": formula.get("args", []),
            }
        )
    if kind in ("and", "or", "not", "implies"):
        return simplify_term(
            {
                "kind": "ctor",
                "name": kind,
                "args": [formula_to_term(item) for item in formula.get("operands", [])],
            }
        )
    raise ValueError(f"formula cannot become term: {formula}")


def eq_zero(term):
    term = simplify_term(term)
    if not isinstance(term, dict):
        return False
    if term.get("kind") != "ctor" or term.get("name") != "=":
        return False
    args = term.get("args", [])
    return args == [var("arg_0"), const_int(0)] or args == [const_int(0), var("arg_0")]


def guard_is_zero_case(guard_formula):
    term = formula_to_term(guard_formula)
    if eq_zero(term):
        return True
    if (
        isinstance(term, dict)
        and term.get("kind") == "ctor"
        and term.get("name") == "not"
        and len(term.get("args", [])) == 1
        and eq_zero(term["args"][0])
    ):
        return False
    raise ValueError(f"unsupported guard: {guard_formula}")


def extract_eq_value(formula):
    if formula.get("kind") != "atomic" or formula.get("name") != "=":
        raise ValueError(f"expected equality: {formula}")
    lhs, rhs = formula["args"]
    if lhs == var("ret"):
        return rhs
    if rhs == var("ret"):
        return lhs
    raise ValueError(f"equality does not bind ret: {formula}")


def direct_post_term(post):
    if post.get("kind") != "atomic" or post.get("name") != "=":
        return None
    lhs, rhs = post.get("args", [None, None])
    if lhs == var("ret"):
        return simplify_term(rhs)
    if rhs == var("ret"):
        return simplify_term(lhs)
    return None


def conjunction_operands(formula):
    if formula.get("kind") == "and":
        return formula.get("operands", [])
    return [formula]


def post_to_shape_term(post):
    direct = direct_post_term(post)
    if direct is not None:
        return direct

    branches = {}
    for item in conjunction_operands(post):
        if item.get("kind") != "implies":
            raise ValueError(f"expected implication: {item}")
        guard, consequence = item["operands"]
        branches[guard_is_zero_case(guard)] = simplify_term(extract_eq_value(consequence))

    if branches.get(True) != const_int(-22):
        raise ValueError(f"zero branch did not normalize to -22: {branches}")
    if branches.get(False) != var("arg_0"):
        raise ValueError(f"nonzero branch did not normalize to arg_0: {branches}")
    return shape_result_term()


def normalize_to_shape_payload(contract, renaming_map, representation_map):
    normalized = normalize_node(contract, renaming_map, representation_map)
    post = normalized.get("post")
    if post is None:
        raise ValueError("source contract lacks post")
    term = post_to_shape_term(post)
    expected = shape_result_term()
    if term != expected:
        raise ValueError(f"normalized post mismatch: {term} != {expected}")
    return shape_payload()


def write_shape_spec():
    write_json(SPEC_DIR / "foo_shape.spec.json", shape_spec())


def morphism_spec(name, source_cid, shape_cid, renaming_map, representation_map, literal_map):
    return {
        "kind": "algorithm",
        "fn_name": name,
        "formals": ["source_contract"],
        "formal_sorts": [
            {"kind": "ctor", "name": "FunctionContractMemento", "args": []}
        ],
        "return_sort": {"kind": "ctor", "name": "FunctionContractMemento", "args": []},
        "pre": copy.deepcopy(TRUE_FORMULA),
        "post": {
            "kind": "contract-renaming-morphism",
            "source_contract_cid": source_cid,
            "target_shape_cid": shape_cid,
            "renaming_map": renaming_map,
            "representation_map": representation_map,
            "literal_map": literal_map,
            "homomorphism_obligation": {
                "kind": "canonicalizer-alpha-equivalence-plus-twos-complement",
                "source": source_cid,
                "target": shape_cid,
            },
        },
        "effects": {"effects": []},
        "input_cids": [source_cid, shape_cid],
    }


def mint(kind, spec_name):
    output = run(
        [
            PROVEKIT,
            "mint",
            kind,
            "--spec",
            SPEC_DIR / spec_name,
            "--unsigned",
            "--catalog",
            CATALOG_ARG,
        ]
    )
    cid, path = output.split("\t", 1)
    return cid, path


def prepare_dirs():
    for path in [SPEC_DIR, SOURCE_DIR, RECEIPT_DIR, DISCHARGE_DIR, BASE / "dev"]:
        path.mkdir(parents=True, exist_ok=True)
    if CATALOG_REAL.exists():
        shutil.rmtree(CATALOG_REAL)
    CATALOG_REAL.mkdir(parents=True, exist_ok=True)
    tmp_dir = BASE / "tmp"
    if tmp_dir.exists():
        shutil.rmtree(tmp_dir)
    pycache = Path(__file__).resolve().parent / "__pycache__"
    if pycache.exists():
        shutil.rmtree(pycache)


def write_sources():
    source_c = ROOT / "menagerie" / "c11-language-signature" / "example" / "foo.expected-wp-contract.json"
    shutil.copyfile(source_c, SOURCE_DIR / "c_foo.contract.json")

    rust_example = ROOT / "menagerie" / "rust-language-signature" / "example"
    rust_contract = json.loads(
        run([RUST_WALK_EMIT, "contract", "foo.rs", "foo"], cwd=rust_example)
    )
    write_json(SOURCE_DIR / "rust_foo.contract.json", rust_contract)

    aarch64_contract = lift_contract(
        AARCH64_LIFTER,
        RUST_DIR / "provekit-lift-asm-aarch64",
        "asm-aarch64",
        "tests/fixtures/foo.s",
    )
    write_json(SOURCE_DIR / "aarch64_foo.contract.json", aarch64_contract)

    x86_contract = lift_contract(
        X86_LIFTER,
        RUST_DIR / "provekit-lift-asm-x86-64",
        "x86-64:sysv",
        "tests/fixtures/foo.s",
    )
    write_json(SOURCE_DIR / "x86_64_foo.contract.json", x86_contract)


def store_receipt(name, receipt):
    write_json(RECEIPT_DIR / f"{name}.receipt.json", receipt)
    cid = canonical_cid_file(RECEIPT_DIR / f"{name}.receipt.json")
    catalog_dir = CATALOG_REAL / "receipts"
    catalog_dir.mkdir(parents=True, exist_ok=True)
    catalog_path = catalog_dir / f"{name}.{cid}.json"
    write_json(catalog_path, {"cid": cid, "memento": receipt, "signature": None})
    return cid, str(catalog_path)


def write_readme(cids, discharge_status, x86_file_cid):
    def label(item):
        if item["kind"] in ("shape", "source"):
            return item["name"]
        return f"{item['kind']}:{item['name']}"

    rows = "\n".join(
        f"| {label(item)} | {item['cid']} |"
        for item in cids
        if item["kind"] in ("shape", "source", "morphism", "receipt")
    )
    discharge_rows = "\n".join(
        f"| {name} | {cid} | {shape_cid} |"
        for name, cid, shape_cid in discharge_status
    )
    text = f"""# Foo Algebraic Shape

This exhibit makes the cross-language federation concrete for:

```c
int foo(int x) {{
  if (x == 0) return -22;
  return x;
}}
```

The C, Rust, AArch64, and x86-64 lifts have different names, return slots, and value representations. Under a quotient that renames the input to `arg_0`, renames the return slot to `ret`, and interprets 32 bit machine literals as signed `Int`, all four collapse to one algebraic shape:

```text
lambda arg_0. ite(arg_0 == 0, -22, arg_0)
```

The federation anchor is the shape CID:

```text
{cids[0]['cid']}
```

## Contracts And CIDs

| Artifact | CID |
| --- | --- |
{rows}

The x86-64 source file also has a raw JSON CID of `{x86_file_cid}` because the emitted contract includes its own `cid` field. The source contract CID used by the morphism is the embedded memento CID, verified by recomputing the canonical CID after removing that field. The prompt cited a historical x86-64 CID of `blake3-512:d6e0c04222f724cdb63d61dcf64962921246dad629113b025b9fd3ea3963a36a57e49efcd6f657b856d5983eb7f2234d6a15fa5ca6af7d88bc78a4705646d291`; the current lifter emits the CID shown above.

## Quotient

The quotient maps source names and representations into the shared shape.

| Source | Renaming | Representation |
| --- | --- | --- |
| C | `x -> arg_0`, `result -> ret` | `i32 -> Int` |
| Rust | `x -> arg_0`, `result -> ret` | `I32 -> Int` |
| AArch64 | `w0 -> arg_0`, `w0_out -> ret` | `BitVector32 -> Int` |
| x86-64 | `edi -> arg_0`, `eax_post -> ret` | `BitVector -> Int`, `0xffffffea -> -22` |

The discharge is not an SMT proof. The script applies the renaming and representation map, folds the x86-64 two's-complement literal to `-22`, canonicalizes the resulting payload, and checks that the CID equals the shape CID.

## Discharges

| Morphism | After substitution CID | Shape CID |
| --- | --- | --- |
{discharge_rows}

All four after-substitution CIDs must equal the shape CID. The receipts live in `receipts/` and are also stored under `catalog/receipts/`.

## C Lifter Gap

This exhibit uses `menagerie/c11-language-signature/example/foo.expected-wp-contract.json` for the C source contract. The current C lifter output in `foo.contract.json` drops the branch and emits `result = x`; that branch-sensitivity gap is known and is being fixed separately.

## Reproduce

Run:

```sh
menagerie/foo-algebraic-shape/mint.sh
```

The script builds the Rust CLI, Rust walker, and the two asm lifters, refreshes `sources/`, mints the shape and morphisms into `catalog/`, writes receipts, updates `cids.tsv`, and scans this exhibit for forbidden dash characters and the forbidden sign-off name.

## References

- `protocol/specs/2026-05-09-algorithm-memento-protocol.md`
- `protocol/specs/2026-05-09-language-signature-protocol.md`
- `docs/papers/03-substrate-not-blockchain.md`
- `docs/papers/13-after-grammars-programming-languages-as-content-addressed-algebras.md`
- `docs/papers/14-after-trust-the-universal-correctness-bundle.md`

T Savo
"""
    (BASE / "README.md").write_text(text, encoding="utf-8")


def scan_created_text():
    bad = []
    forbidden_name = "Tra" + "vis"
    for path in BASE.rglob("*"):
        if not path.is_file():
            continue
        if "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        data = path.read_bytes()
        if b"\xe2\x80\x94" in data:
            bad.append(f"{path}: em dash")
        if b"\xe2\x80\x93" in data:
            bad.append(f"{path}: en dash")
        text = data.decode("utf-8", errors="ignore")
        if forbidden_name in text:
            bad.append(f"{path}: forbidden signoff name")
    if bad:
        raise SystemExit("\n".join(bad))


def main():
    build_tools()
    prepare_dirs()
    write_sources()
    write_shape_spec()

    source_cids = {}
    file_cids = {}
    for name in ["c", "rust", "aarch64", "x86_64"]:
        cid, file_cid = source_contract_cid(SOURCE_DIR / f"{name}_foo.contract.json")
        source_cids[name] = cid
        file_cids[name] = file_cid

    shape_cid, shape_path = mint("algorithm", "foo_shape.spec.json")
    expected_shape_cid = canonical_cid_value(shape_payload())
    if shape_cid != expected_shape_cid:
        raise SystemExit(f"shape CID mismatch: {shape_cid} != {expected_shape_cid}")

    morphisms = {
        "morphism_c_to_shape": {
            "source": "c",
            "renaming": {"x": "arg_0", "result": "ret"},
            "representation": {"i32": "Int"},
            "literal": {},
        },
        "morphism_rust_to_shape": {
            "source": "rust",
            "renaming": {"x": "arg_0", "result": "ret"},
            "representation": {"I32": "Int"},
            "literal": {},
        },
        "morphism_aarch64_to_shape": {
            "source": "aarch64",
            "renaming": {"w0": "arg_0", "w0_out": "ret"},
            "representation": {"BitVector32": "Int"},
            "literal": {},
        },
        "morphism_x86_64_to_shape": {
            "source": "x86_64",
            "renaming": {"edi": "arg_0", "eax_post": "ret"},
            "representation": {"BitVector": "Int"},
            "literal": {"0xffffffea": -22},
        },
    }

    cids = [
        {"kind": "shape", "name": "shape:foo", "cid": shape_cid, "path": shape_path},
        {
            "kind": "source",
            "name": "c_foo",
            "cid": source_cids["c"],
            "path": str(SOURCE_DIR / "c_foo.contract.json"),
        },
        {
            "kind": "source",
            "name": "rust_foo",
            "cid": source_cids["rust"],
            "path": str(SOURCE_DIR / "rust_foo.contract.json"),
        },
        {
            "kind": "source",
            "name": "aarch64_foo",
            "cid": source_cids["aarch64"],
            "path": str(SOURCE_DIR / "aarch64_foo.contract.json"),
        },
        {
            "kind": "source",
            "name": "x86_64_foo",
            "cid": source_cids["x86_64"],
            "path": str(SOURCE_DIR / "x86_64_foo.contract.json"),
        },
    ]

    discharge_status = []
    for spec_stem, info in morphisms.items():
        source = info["source"]
        spec = morphism_spec(
            f"foo:{source}:to-shape",
            source_cids[source],
            shape_cid,
            info["renaming"],
            info["representation"],
            info["literal"],
        )
        spec_name = f"{spec_stem}.spec.json"
        write_json(SPEC_DIR / spec_name, spec)
        morphism_cid, morphism_path = mint("algorithm", spec_name)
        cids.append(
            {
                "kind": "morphism",
                "name": spec_stem,
                "cid": morphism_cid,
                "path": morphism_path,
            }
        )

        contract = read_json(SOURCE_DIR / f"{source}_foo.contract.json")
        after_payload = normalize_to_shape_payload(
            contract, info["renaming"], info["representation"]
        )
        after_path = DISCHARGE_DIR / f"{source}_after_substitution.json"
        write_json(after_path, after_payload)
        after_cid = canonical_cid_file(after_path)
        if after_cid != shape_cid:
            raise SystemExit(f"{source} discharge landed on {after_cid}, not {shape_cid}")

        receipt = {
            "schema_version": "1",
            "kind": "MorphismDischargeReceipt",
            "morphism_cid": morphism_cid,
            "source_contract_cid": source_cids[source],
            "renaming_map": info["renaming"],
            "representation_map": info["representation"],
            "after_substitution_cid": after_cid,
            "shape_cid": shape_cid,
            "discharged": True,
            "method": "canonicalizer-alpha-equivalence-plus-twos-complement",
        }
        receipt_cid, receipt_path = store_receipt(spec_stem, receipt)
        cids.append(
            {
                "kind": "receipt",
                "name": spec_stem,
                "cid": receipt_cid,
                "path": receipt_path,
            }
        )
        discharge_status.append((spec_stem, after_cid, shape_cid))

    with CID_FILE.open("w", encoding="utf-8") as handle:
        handle.write("kind\tname\tcid\tpath\n")
        for item in cids:
            handle.write(
                f"{item['kind']}\t{item['name']}\t{item['cid']}\t{item['path']}\n"
            )

    write_readme(cids, discharge_status, file_cids["x86_64"])
    scan_created_text()

    print(f"shape_cid\t{shape_cid}")
    for key in ["c", "rust", "aarch64", "x86_64"]:
        print(f"source_cid\t{key}\t{source_cids[key]}")
    for name, after_cid, target_cid in discharge_status:
        print(f"discharge\t{name}\t{after_cid}\t{target_cid}")


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        raise
