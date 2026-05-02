# SPDX-License-Identifier: Apache-2.0
#
# Cross-language conformance: every JCS byte and BLAKE3-512 hash this
# kit emits MUST match the canonical fixtures shared by every other
# ProvekIt implementation. Source of truth: ../../conformance/fixtures.toml.

require "minitest/autorun"
require_relative "../lib/provekit/ir"

class TestJcsConformance < Minitest::Test
  IR = Provekit::IR

  # Fixture: eq_atomic — parse_int('42') = 42
  EXPECTED_EQ_ATOMIC =
    '{"args":[{"args":[{"kind":"const","sort":{"kind":"primitive","name":"String"},"value":"42"}],"kind":"ctor","name":"parse_int"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":42}],"kind":"atomic","name":"="}'

  # Fixture: pattern1_bounded_loop — forall x: (x ≥ 0 ∧ x < 100) ⇒ x ≥ 0
  EXPECTED_PATTERN1 =
    '{"body":{"kind":"implies","operands":[{"kind":"and","operands":[{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":100}],"kind":"atomic","name":"<"}]},{"args":[{"kind":"var","name":"x"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":0}],"kind":"atomic","name":"≥"}]},"kind":"forall","name":"x","sort":{"kind":"primitive","name":"Int"}}'

  def test_eq_atomic
    formula = IR.atomic("=",
      IR.ctor("parse_int", IR.str("42")),
      IR.num(42))
    assert_equal EXPECTED_EQ_ATOMIC, IR::Jcs.encode(formula)
  end

  def test_pattern1_bounded_loop
    x = IR.var(name: "x")
    body = IR.implies(
      IR.and(IR.gte(x, IR.num(0)), IR.lt(x, IR.num(100))),
      IR.gte(x, IR.num(0)))
    formula = IR.forall(name: "x", sort: IR::Sort.Int, body: body)
    assert_equal EXPECTED_PATTERN1, IR::Jcs.encode(formula)
  end
end
