#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[4]
FIXTURES_DIR = REPO_ROOT / "implementations" / "c" / "conformance" / "fixtures"
BIND_LIFT_BIN = REPO_ROOT / "implementations" / "c" / "provekit-walk-c" / "provekit-bind-lift-c"
REQUIRED_FIXTURES = [
    "hello_world",
    "recursive_factorial",
    "arithmetic_add",
    "arithmetic_multi_op",
    "control_flow_if",
    "transported_op_concept_comment",
]
ZERO_CID = "blake3-512:" + "0" * 128
ONE_CID = "blake3-512:" + "1" * 128


class ConformanceRefusal(Exception):
    def __init__(self, failure_kind, failure_detail):
        super().__init__(failure_detail)
        self.failure_kind = failure_kind
        self.failure_detail = failure_detail

    def to_memento(self):
        return {
            "envelope": {
                "declaredAt": "1970-01-01T00:00:00Z",
                "signature": "unsigned:c-kit-conformance",
                "signer": "substrate:c-kit-conformance",
            },
            "header": {
                "atoms_cids": [ONE_CID],
                "blocking_effects": None,
                "ccp_version": "1.0.0",
                "cid": ZERO_CID,
                "compose_input_cid": ZERO_CID,
                "effect_occurrences": [],
                "effect_set_cids": [],
                "failure_detail": self.failure_detail,
                "failure_kind": self.failure_kind,
                "incompatible_pair": None,
                "kind": "composition-refusal",
                "missing_memento_requirements": None,
                "schemaVersion": "1",
            },
            "metadata": {
                "note": "C kit emit-compile-run conformance refusal",
            },
        }


def load_fixtures():
    fixtures_by_name = {}
    for path in sorted(FIXTURES_DIR.glob("*.json")):
        with path.open("r", encoding="utf-8") as handle:
            fixture = json.load(handle)
        fixture["_path"] = str(path)
        fixtures_by_name[fixture["name"]] = fixture
    missing = [name for name in REQUIRED_FIXTURES if name not in fixtures_by_name]
    if missing:
        raise RuntimeError(f"missing required C conformance fixtures: {missing}")
    return [fixtures_by_name[name] for name in REQUIRED_FIXTURES]


def rpc_lines(cmd, payload):
    proc = subprocess.run(
        cmd,
        input=payload,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"{cmd} exited {proc.returncode}: stdout={proc.stdout!r} stderr={proc.stderr!r}"
        )
    return [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]


def lift_original_source(tmp, fixture):
    if not BIND_LIFT_BIN.exists():
        raise RuntimeError(f"missing C bind lifter: {BIND_LIFT_BIN}")
    source_path = tmp / f"{fixture['name']}_lift.c"
    source_path.write_text(fixture["original_source"], encoding="utf-8")
    request = "\n".join(
        [
            json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            json.dumps(
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "lift",
                    "params": {
                        "workspace_root": str(tmp),
                        "source_paths": [source_path.name],
                    },
                }
            ),
            json.dumps({"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}}),
        ]
    )
    responses = rpc_lines([str(BIND_LIFT_BIN)], request + "\n")
    result = responses[1].get("result", {})
    if result.get("kind") != "ir-document":
        raise RuntimeError(f"C bind lifter returned non-ir document: {result}")
    target_fn = fixture["realize_request"]["function"]
    matches = [entry for entry in result.get("ir", []) if entry.get("fn_name") == target_fn]
    if len(matches) != 1:
        raise RuntimeError(f"expected one lifted entry for {target_fn}, got {matches}")
    return matches[0]


def request_from_lift(fixture, entry):
    request = dict(fixture["realize_request"])
    expected = {
        "function": entry.get("fn_name"),
        "params": entry.get("param_names", []),
        "param_types": entry.get("param_types", []),
        "return_type": entry.get("return_type"),
    }
    for key, lifted in expected.items():
        fixture_value = request.get(key)
        if key == "return_type" and void_equivalent(fixture_value, lifted):
            continue
        if fixture_value != lifted:
            raise RuntimeError(
                f"{fixture['name']} {key} mismatch: fixture={fixture_value!r} lifted={lifted!r}"
            )
    if request.get("concept_name") in (None, ""):
        concept = entry.get("concept_annotation")
        if not concept:
            raise RuntimeError(f"{fixture['name']} has no lifted concept annotation")
        request["concept_name"] = concept
    return request


def void_equivalent(left, right):
    return left in ("void", "()", "Unit") and right in ("void", "()", "Unit")


def invoke_realizer(bin_path, request):
    rpc = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.invoke",
        "params": request,
    }
    responses = rpc_lines(
        [str(bin_path), "--rpc"],
        json.dumps(rpc, separators=(",", ":")) + "\n",
    )
    response = responses[0]
    if "error" in response:
        raise RuntimeError(f"realizer returned error: {response['error']}")
    result = response.get("result", {})
    source = result.get("source")
    if not isinstance(source, str):
        raise RuntimeError(f"realizer response missing result.source: {response}")
    return source


def compile_source(cc, source_path, out_path, fixture_name):
    cmd = [cc, "-Wall", "-Wextra", "-Werror", str(source_path), "-o", str(out_path)]
    proc = subprocess.run(
        cmd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        detail = {
            "fixture": fixture_name,
            "command": cmd,
            "stdout": proc.stdout,
            "stderr": proc.stderr,
        }
        raise ConformanceRefusal(
            "target-compile-failure",
            json.dumps(detail, sort_keys=True, separators=(",", ":")),
        )


def run_binary(bin_path, argv):
    proc = subprocess.run(
        [str(bin_path), *[str(arg) for arg in argv]],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    return {
        "stdout": proc.stdout,
        "stderr": proc.stderr,
        "exit_code": proc.returncode,
    }


def divergence(fixture_name, case_index, expected, observed, label):
    detail = {
        "fixture": fixture_name,
        "case_index": case_index,
        "comparison": label,
        "expected": expected,
        "observed": observed,
    }
    raise ConformanceRefusal(
        "target-behavior-divergence",
        json.dumps(detail, sort_keys=True, separators=(",", ":")),
    )


def int_driver(function_name, params):
    argc = len(params) + 1
    declarations = []
    for index, name in enumerate(params):
        declarations.append(
            f'    int arg{index} = parse_int_arg(argv[{index + 1}], "{name}");'
        )
    call_args = ", ".join(f"arg{index}" for index in range(len(params)))
    return f"""
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>

static int parse_int_arg(const char *raw, const char *name) {{
    char *end = NULL;
    errno = 0;
    long value = strtol(raw, &end, 10);
    if (errno != 0 || end == raw || *end != '\\0' ||
        value < INT_MIN || value > INT_MAX) {{
        fprintf(stderr, "invalid integer for %s: %s\\n", name, raw);
        exit(64);
    }}
    return (int)value;
}}

int main(int argc, char **argv) {{
    if (argc != {argc}) {{
        fprintf(stderr, "expected {len(params)} args, got %d\\n", argc - 1);
        return 64;
    }}
{os.linesep.join(declarations)}
    int result = {function_name}({call_args});
    printf("%d\\n", result);
    return 0;
}}
"""


def void_driver(function_name):
    return f"""
int main(void) {{
    {function_name}(0);
    return 0;
}}
"""


def execution_unit(fixture, source):
    kind = fixture["kind"]
    if kind == "program":
        return source
    request = fixture["realize_request"]
    if kind == "function_int":
        return source + "\n" + int_driver(request["function"], request["params"])
    if kind == "concept_carrier":
        return source + "\n" + void_driver(request["function"])
    raise RuntimeError(f"unknown fixture kind {kind!r}")


def assert_expected_outputs(fixture, original_obs, emitted_obs):
    for index, expected in enumerate(fixture["expected_output"]):
        expected_run = {
            "stdout": expected.get("stdout", ""),
            "stderr": expected.get("stderr", ""),
            "exit_code": expected.get("exit_code", 0),
        }
        if original_obs[index] != expected_run:
            divergence(
                fixture["name"],
                index,
                expected_run,
                original_obs[index],
                "original-vs-expected",
            )
        if emitted_obs[index] != original_obs[index]:
            divergence(
                fixture["name"],
                index,
                original_obs[index],
                emitted_obs[index],
                "emitted-vs-original",
            )


def run_executable_fixture(cc, tmp, fixture, emitted_source):
    original_c = tmp / f"{fixture['name']}_original.c"
    emitted_c = tmp / f"{fixture['name']}_emitted.c"
    original_bin = tmp / f"{fixture['name']}_original"
    emitted_bin = tmp / f"{fixture['name']}_emitted"

    original_c.write_text(
        execution_unit(fixture, fixture["original_source"]), encoding="utf-8"
    )
    emitted_c.write_text(execution_unit(fixture, emitted_source), encoding="utf-8")

    compile_source(cc, original_c, original_bin, fixture["name"])
    compile_source(cc, emitted_c, emitted_bin, fixture["name"])

    original_obs = []
    emitted_obs = []
    for argv in fixture["declared_test_inputs"]:
        original_obs.append(run_binary(original_bin, argv))
        emitted_obs.append(run_binary(emitted_bin, argv))
    assert_expected_outputs(fixture, original_obs, emitted_obs)


def run_carrier_fixture(cc, tmp, fixture, emitted_source):
    expected = fixture["expected_output"][0]
    for needle in expected["source_contains"]:
        if needle not in emitted_source:
            divergence(
                fixture["name"],
                0,
                {"source_contains": needle},
                {"source": emitted_source},
                "carrier-comment-survival",
            )
    run_executable_fixture(cc, tmp, fixture, emitted_source)


def run_fixture(cc, bin_path, tmp, fixture):
    lifted = lift_original_source(tmp, fixture)
    request = request_from_lift(fixture, lifted)
    emitted_source = invoke_realizer(bin_path, request)
    if fixture["kind"] == "concept_carrier":
        run_carrier_fixture(cc, tmp, fixture, emitted_source)
    else:
        run_executable_fixture(cc, tmp, fixture, emitted_source)


def verify_refusal_paths(cc, tmp):
    bad_source = tmp / "compile_refusal_probe.c"
    bad_source.write_text("int main(void) { return ; }\n", encoding="utf-8")
    try:
        compile_source(cc, bad_source, tmp / "compile_refusal_probe", "compile_refusal_probe")
    except ConformanceRefusal as refusal:
        if refusal.failure_kind != "target-compile-failure":
            raise
    else:
        raise RuntimeError("compile refusal probe unexpectedly compiled")

    try:
        divergence(
            "behavior_refusal_probe",
            0,
            {"stdout": "1\n", "stderr": "", "exit_code": 0},
            {"stdout": "2\n", "stderr": "", "exit_code": 0},
            "refusal-probe",
        )
    except ConformanceRefusal as refusal:
        if refusal.failure_kind != "target-behavior-divergence":
            raise
    else:
        raise RuntimeError("behavior refusal probe did not raise")


def main():
    if len(sys.argv) != 2:
        print("usage: conformance.py <provekit-realize-c>", file=sys.stderr)
        return 2
    bin_path = Path(sys.argv[1]).resolve()
    if not bin_path.exists():
        print(f"missing executable: {bin_path}", file=sys.stderr)
        return 2
    cc = os.environ.get("CC", "cc")
    fixtures = load_fixtures()
    try:
        with tempfile.TemporaryDirectory(prefix="provekit-c-conformance-") as tmp_raw:
            tmp = Path(tmp_raw)
            verify_refusal_paths(cc, tmp)
            for fixture in fixtures:
                run_fixture(cc, bin_path, tmp, fixture)
    except ConformanceRefusal as refusal:
        print(json.dumps(refusal.to_memento(), indent=2, sort_keys=True), file=sys.stderr)
        return 1
    print(f"C emit-compile-run conformance: {len(fixtures)} fixtures")
    print("C refusal probes: target-compile-failure target-behavior-divergence")
    return 0


if __name__ == "__main__":
    sys.exit(main())
