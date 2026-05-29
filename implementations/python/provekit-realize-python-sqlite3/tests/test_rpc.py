from __future__ import annotations

import base64
import json
import py_compile
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
CORE_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"
if str(CORE_SRC) not in sys.path:
    sys.path.insert(0, str(CORE_SRC))
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-sqlite3/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_sqlite3.rpc import dispatch


def test_rpc_assemble_returns_compileable_python_module(tmp_path: Path) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "provekit.plugin.assemble",
            "params": {
                "target_lang": "python",
                "file_basename": "queries",
                "fragments": [
                    {
                        "concept_name": "concept:sql-query-all",
                        "source": (
                            "def select_rows(db, sql, args):\n"
                            "    cursor = db.execute(sql, tuple(args))\n"
                            "    return cursor.fetchall()\n"
                        ),
                        "imports": ["sqlite3"],
                        "helpers": ["DEFAULT_LIMIT = 100"],
                    }
                ],
            },
        }
    )

    assert "error" not in response
    file = response["result"]["files"][0]
    assert file["path"] == "queries.py"
    content = file["content"]
    assert "import sqlite3" in content
    assert "DEFAULT_LIMIT = 100" in content
    assert "def select_rows(db, sql, args):" in content

    module = tmp_path / "queries.py"
    module.write_text(content, encoding="utf-8")
    py_compile.compile(str(module), doraise=True)


def test_rpc_invoke_renders_sqlite3_query_all_body(disk_fixture) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "select_rows",
                "params": ["sql", "args"],
                "param_types": ["str", "list[object]"],
                "return_type": "list[object]",
                "concept_name": "concept:sql-query-all",
            },
        }
    )

    assert response["id"] == 1
    assert response["result"]["is_stub"] is False
    assert "db.execute" in response["result"]["source"]


def test_rpc_invoke_uses_named_term_tree_shape_for_sql_query_all(disk_fixture) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "select_by_id",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "list[object]",
                "concept_name": "concept:sql-query-all",
                "named_term_tree": {
                    "conceptName": "concept:sql-query-all",
                    "operationKind": "sql-query-all",
                    "shapeCid": "blake3-512:sql-query-all",
                    "args": [
                        {
                            "conceptName": "Sql",
                            "operationKind": "literal",
                            "shapeCid": "blake3-512:sql",
                            "args": [],
                        },
                        {
                            "conceptName": "SqlArgs",
                            "operationKind": "tuple",
                            "shapeCid": "blake3-512:sql-args",
                            "args": [],
                        },
                    ],
                },
            },
        }
    )

    assert response["id"] == 3
    assert response["result"]["source"] == (
        "def select_by_id(id):\n"
        "    cursor = db.execute(sql, tuple(args))\n"
        "    return cursor.fetchall()\n"
    )
    assert response["result"]["is_stub"] is False


def test_rpc_invoke_without_named_term_tree_keeps_bare_signature_lookup(
    disk_fixture,
) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "select_by_id",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "list[object]",
                "concept_name": "missing-concept",
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 4,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["int"],
                    "function": "select_by_id",
                    "term_position": "body",
                }
            ],
        },
    }


def test_rpc_missing_template_reports_named_term_tree_shape() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "missing_query",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "list[object]",
                "concept_name": "missing-concept",
                "named_term_tree": {
                    "conceptName": "missing-concept",
                    "operationKind": "missing",
                    "shapeCid": "blake3-512:missing",
                    "args": [
                        {
                            "conceptName": "Sql",
                            "operationKind": "literal",
                            "shapeCid": "blake3-512:sql",
                            "args": [],
                        },
                        {
                            "conceptName": "SqlArgs",
                            "operationKind": "tuple",
                            "shapeCid": "blake3-512:sql-args",
                            "args": [],
                        },
                    ],
                },
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 5,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["str", "list[object]"],
                    "function": "missing_query",
                    "term_position": "body",
                }
            ],
        },
    }


def test_plugin_invoke_returns_structured_missing_template_error() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "missing",
                "params": ["x"],
                "param_types": ["int"],
                "return_type": "int",
                "concept_name": "missing-concept",
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 7,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["int"],
                    "function": "missing",
                    "term_position": "body",
                }
            ],
        },
    }


def test_emit_module_returns_all_missing_template_errors() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 8,
            "method": "provekit.plugin.emit_module",
            "params": {
                "functions": [
                    {
                        "function": "first",
                        "params": ["x"],
                        "param_types": ["int"],
                        "return_type": "int",
                        "concept_name": "first-missing",
                    },
                    {
                        "function": "second",
                        "params": ["y"],
                        "param_types": ["str"],
                        "return_type": "str",
                        "concept_name": "second-missing",
                    },
                ]
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 8,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "first-missing",
                    "args_shape": ["int"],
                    "function": "first",
                    "term_position": "body",
                },
                {
                    "operation_kind": "second-missing",
                    "args_shape": ["str"],
                    "function": "second",
                    "term_position": "body",
                },
            ],
        },
    }


def test_rpc_error_for_unknown_method_is_json_serializable() -> None:
    response = dispatch({"jsonrpc": "2.0", "id": 2, "method": "missing"})

    assert response["error"]["code"] == -32601
    json.dumps(response)


def test_resolve_dependency_proofs_returns_distribution_proof_bytes(
    tmp_path: Path, monkeypatch
) -> None:
    site_packages = tmp_path / "site-packages"
    site_packages.mkdir()
    proof_path = _install_fake_distribution(site_packages, "sqlite_dep", "d")
    monkeypatch.syspath_prepend(str(site_packages))

    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "provekit.plugin.resolve_dependency_proofs",
            "params": {"project_root": str(tmp_path / "project")},
        }
    )

    assert "error" not in response, response
    proof = response["result"]["proofs"][0]
    assert proof["cid"] == proof_path.name.removesuffix(".proof")
    assert base64.b64decode(proof["bytes_base64"]) == proof_path.read_bytes()
    assert proof["source"] == f"python-distribution:{proof_path.name}"


def _install_fake_distribution(
    site_packages: Path,
    distribution_name: str,
    proof_digit: str,
) -> Path:
    package_name = distribution_name.replace("-", "_")
    package_dir = site_packages / package_name
    dist_info = site_packages / f"{distribution_name}-1.0.dist-info"
    package_dir.mkdir()
    dist_info.mkdir()

    (package_dir / "__init__.py").write_text("", encoding="utf-8")
    proof_name = f"blake3-512:{proof_digit * 128}.proof"
    proof_path = package_dir / proof_name
    proof_path.write_text(f"synthetic proof for {distribution_name}\n", encoding="utf-8")

    (dist_info / "METADATA").write_text(
        f"Metadata-Version: 2.1\nName: {distribution_name}\nVersion: 1.0\n",
        encoding="utf-8",
    )
    (dist_info / "WHEEL").write_text("Wheel-Version: 1.0\n", encoding="utf-8")
    (dist_info / "RECORD").write_text(
        "\n".join(
            [
                f"{package_name}/__init__.py,,",
                f"{package_name}/{proof_name},,",
                f"{distribution_name}-1.0.dist-info/METADATA,,",
                f"{distribution_name}-1.0.dist-info/WHEEL,,",
                f"{distribution_name}-1.0.dist-info/RECORD,,",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    return proof_path
