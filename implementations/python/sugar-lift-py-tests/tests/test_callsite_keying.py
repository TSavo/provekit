# SPDX-License-Identifier: Apache-2.0
#
# The contract ties to the CALLSITE UNDER TEST, not the enclosing test.
#
# A module-function call (`np.add(2,3)`) keys to the qualified callsite
# `numpy.add#euf#<args>` -- alias-resolved (np -> numpy) and argument-keyed -- so
# two assertions about the SAME call, in DIFFERENT tests or files, conjoin and a
# contradiction fires UNSAT. Discrimination tests guard the soundness boundary:
# the catch must fire, AND the false-refusals must NOT (different args, symbolic
# args, and receiver-dependent method calls all stay independent).

from sugar_lift_py_tests import layer2


def _names(src: str):
    return [d.name for d in layer2.lift_file_layer2(src, "t.py").decls]


def _euf_base(name: str) -> str:
    # drop the ::facts / ::assertion suffix to compare the callsite base
    return name.split("::")[0]


def test_module_call_keys_to_callsite_not_test():
    src = (
        "import numpy as np\n"
        "def test_user():\n"
        "    r = np.add(2, 3)\n"
        "    assert r == 6\n"
    )
    bases = {_euf_base(n) for n in _names(src)}
    # tied to the callsite (numpy.add applied to (2,3)), NOT to `test_user`
    assert bases == {"numpy.add#euf#c:callresult_numpy_add_a2(i:2,i:3)"}, bases


def test_alias_and_qualified_key_identically():
    # `np.add(2,3)` and `numpy.add(2,3)` must produce the SAME callsite key
    np_src = "import numpy as np\ndef test_a():\n    assert np.add(2, 3) == 5\n"
    full_src = "import numpy\ndef test_b():\n    assert numpy.add(2, 3) == 6\n"
    np_base = {_euf_base(n) for n in _names(np_src)}
    full_base = {_euf_base(n) for n in _names(full_src)}
    assert np_base == full_base, (np_base, full_base)


def test_different_args_key_differently():
    # np.add(2,3) and np.add(2,4) are DIFFERENT callsites -> different keys ->
    # no false unification (would otherwise refuse a perfectly fine pair).
    a = {_euf_base(n) for n in _names(
        "import numpy as np\ndef test_a():\n    assert np.add(2, 3) == 5\n")}
    b = {_euf_base(n) for n in _names(
        "import numpy as np\ndef test_b():\n    assert np.add(2, 4) == 6\n")}
    assert a.isdisjoint(b), (a, b)


def test_method_call_on_object_is_not_callsite_keyed():
    # A method on a non-module receiver is RECEIVER-DEPENDENT: it must NOT key to
    # a shared callsite (else two objects' `.compute(5)` would falsely unify).
    src = (
        "def test_obj():\n"
        "    obj = make()\n"
        "    r = obj.compute(5)\n"
        "    assert r == 1\n"
    )
    bases = {_euf_base(n) for n in _names(src)}
    assert not any(b.startswith("compute#euf#") or "#euf#" in b for b in bases), bases


def test_dotted_import_resolves_to_top_level_package():
    # `import numpy.linalg` binds the TOP-LEVEL name `numpy` to the numpy package,
    # NOT to numpy.linalg. `numpy.add(2,3)` must key to `numpy.add`, never
    # `numpy.linalg.add`. (Review: CodeRabbit major on dotted imports.)
    src = (
        "import numpy.linalg\n"
        "def test_d():\n"
        "    assert numpy.add(2, 3) == 5\n"
    )
    bases = {_euf_base(n) for n in _names(src)}
    assert bases == {"numpy.add#euf#c:callresult_numpy_add_a2(i:2,i:3)"}, bases


def test_symbolic_args_do_not_unify():
    # np.add(x, y) with symbolic args must NOT be argument-keyed (x, y bind
    # independently per function); stays location-keyed -> no cross-test unify.
    src = (
        "import numpy as np\n"
        "def test_sym(x, y):\n"
        "    r = np.add(x, y)\n"
        "    assert r == 1\n"
    )
    bases = {_euf_base(n) for n in _names(src)}
    assert not any("#euf#" in b for b in bases), bases
