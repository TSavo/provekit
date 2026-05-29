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
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-aiosqlite/src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))

from provekit_realize_python_aiosqlite.rpc import dispatch


SQL_QUERY_ALL_NTT = {
    "conceptName": "concept:sql-query-all",
    "operationKind": "op-application",
    "shapeCid": "blake3-512:sql-query-all",
    "args": [
        {
            "conceptName": "Sql",
            "operationKind": "const",
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
}


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
                            "async def select_rows(db, sql, args):\n"
                            "    async with db.execute(sql, tuple(args)) as cursor:\n"
                            "        return await cursor.fetchall()\n"
                        ),
                        "imports": ["aiosqlite"],
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
    assert "import aiosqlite" in content
    assert "DEFAULT_LIMIT = 100" in content
    assert "async def select_rows(db, sql, args):" in content

    module = tmp_path / "queries.py"
    module.write_text(content, encoding="utf-8")
    py_compile.compile(str(module), doraise=True)


def test_rpc_invoke_renders_aiosqlite_query_all_body(disk_fixture) -> None:
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
    assert "async with db.execute" in response["result"]["source"]


def test_rpc_invoke_threads_named_term_tree_for_template_lookup(disk_fixture) -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 9,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "get_user_by_id",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "User",
                "concept_name": "concept:sql-query-all",
                "named_term_tree": SQL_QUERY_ALL_NTT,
            },
        }
    )

    assert response["id"] == 9
    assert response["result"]["is_stub"] is False
    assert response["result"]["source"] == (
        "async def get_user_by_id(id):\n"
        "    async with db.execute(sql, tuple(args)) as cursor:\n"
        "        return await cursor.fetchall()\n"
    )


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


def test_plugin_invoke_missing_template_uses_named_term_tree_args_shape() -> None:
    response = dispatch(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "provekit.plugin.invoke",
            "params": {
                "function": "missing",
                "params": ["id"],
                "param_types": ["int"],
                "return_type": "User",
                "concept_name": "missing-concept",
                "named_term_tree": SQL_QUERY_ALL_NTT,
            },
        }
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 10,
        "error": {
            "code": -32100,
            "message": "missing body-template entry",
            "data": [
                {
                    "operation_kind": "missing-concept",
                    "args_shape": ["str", "list[object]"],
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
    proof_path = _install_fake_distribution(site_packages, "aiosqlite_dep", "e")
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
