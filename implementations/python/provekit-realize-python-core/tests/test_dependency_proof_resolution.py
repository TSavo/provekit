from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[4]
PKG_SRC = ROOT / "implementations/python/provekit-realize-python-core/src"


def test_resolve_dependency_proofs_returns_installed_distribution_proofs(tmp_path: Path) -> None:
    site_packages = tmp_path / "site-packages"
    site_packages.mkdir()
    first_proof = _install_fake_distribution(site_packages, "voltron_dep_alpha", "a")
    second_proof = _install_fake_distribution(site_packages, "voltron_dep_beta", "b")
    project_root = tmp_path / "project"
    project_root.mkdir()

    response = _rpc_call(
        site_packages,
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "provekit.plugin.resolve_dependency_proofs",
            "params": {"project_root": str(project_root)},
        },
    )

    assert "error" not in response, response
    proof_paths = response["result"]["proof_paths"]
    assert set(proof_paths) == {str(first_proof), str(second_proof)}
    for proof_path in proof_paths:
        path = Path(proof_path)
        assert path.is_absolute()
        assert path.is_file()
        assert path.read_text(encoding="utf-8").startswith("synthetic proof for")


def test_resolve_dependency_proofs_returns_empty_array_without_distribution_proofs(
    tmp_path: Path,
) -> None:
    site_packages = tmp_path / "site-packages"
    site_packages.mkdir()
    _install_fake_distribution(site_packages, "plain_dep", None)
    project_root = tmp_path / "project"
    project_root.mkdir()

    response = _rpc_call(
        site_packages,
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "provekit.plugin.resolve_dependency_proofs",
            "params": {"project_root": str(project_root)},
        },
    )

    assert response == {
        "jsonrpc": "2.0",
        "id": 4,
        "result": {"proof_paths": []},
    }


def _install_fake_distribution(
    site_packages: Path,
    distribution_name: str,
    proof_digit: str | None,
) -> Path | None:
    package_name = distribution_name.replace("-", "_")
    package_dir = site_packages / package_name
    dist_info = site_packages / f"{distribution_name}-1.0.dist-info"
    package_dir.mkdir()
    dist_info.mkdir()

    init_path = package_dir / "__init__.py"
    init_path.write_text("", encoding="utf-8")
    resource_path = package_dir / "not-a-proof.txt"
    resource_path.write_text("ordinary package data\n", encoding="utf-8")

    record_entries = [
        f"{package_name}/__init__.py,,",
        f"{package_name}/not-a-proof.txt,,",
        f"{distribution_name}-1.0.dist-info/METADATA,,",
        f"{distribution_name}-1.0.dist-info/WHEEL,,",
        f"{distribution_name}-1.0.dist-info/RECORD,,",
    ]

    proof_path = None
    if proof_digit is not None:
        proof_name = f"blake3-512:{proof_digit * 128}.proof"
        proof_path = package_dir / proof_name
        proof_path.write_text(f"synthetic proof for {distribution_name}\n", encoding="utf-8")
        record_entries.append(f"{package_name}/{proof_name},,")

    (dist_info / "METADATA").write_text(
        f"Metadata-Version: 2.1\nName: {distribution_name}\nVersion: 1.0\n",
        encoding="utf-8",
    )
    (dist_info / "WHEEL").write_text("Wheel-Version: 1.0\n", encoding="utf-8")
    (dist_info / "RECORD").write_text("\n".join(record_entries) + "\n", encoding="utf-8")
    return proof_path


def _rpc_call(site_packages: Path, request: dict[str, Any]) -> dict[str, Any]:
    env = os.environ.copy()
    env["PYTHONPATH"] = os.pathsep.join([str(PKG_SRC), str(site_packages)])
    env["PYTHONNOUSERSITE"] = "1"
    shutdown = {"jsonrpc": "2.0", "id": "shutdown", "method": "provekit.plugin.shutdown"}
    proc = subprocess.run(
        [sys.executable, "-S", "-m", "provekit_realize_python_core", "--rpc"],
        input=json.dumps(request) + "\n" + json.dumps(shutdown) + "\n",
        capture_output=True,
        text=True,
        check=False,
        env=env,
    )
    assert proc.returncode == 0, proc.stderr
    lines = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
    assert lines, proc.stderr
    return lines[0]
