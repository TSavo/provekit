from sugar_check.__main__ import treeish, manifest_toml, CONFIG_TOML, KIT_MODULE


def test_treeish_whole_tree_vs_subtree():
    assert treeish("HEAD", ".") == "HEAD"
    assert treeish("HEAD", "") == "HEAD"
    assert treeish("v1.2.3", "pkg") == "v1.2.3:pkg"


def test_config_declares_pytest_surface():
    assert 'surface = "python-tests"' in CONFIG_TOML
    assert "[solvers]" in CONFIG_TOML


def test_manifest_targets_the_pytest_lifter():
    m = manifest_toml()
    assert KIT_MODULE in m
    assert 'authoring_surfaces = ["python-tests"]' in m
