# SPDX-License-Identifier: Apache-2.0
#
# Deterministic CBOR encoder tests. RFC 8949 §4.2.1 rules:
#   - shortest-form integer encoding
#   - definite-length items only
#   - map keys in bytewise lex order of their CBOR-encoded form
#
# Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs tests.

require "minitest/autorun"
require_relative "../lib/provekit/cbor"

class TestCbor < Minitest::Test
  C = Provekit::Cbor

  def test_shortest_form_uint_zero
    o = []
    C.encode_uint(o, 0)
    assert_equal [0x00], o
  end

  def test_shortest_form_uint_23
    o = []
    C.encode_uint(o, 23)
    assert_equal [0x17], o
  end

  def test_shortest_form_uint_24
    o = []
    C.encode_uint(o, 24)
    assert_equal [0x18, 24], o
  end

  def test_shortest_form_uint_256
    o = []
    C.encode_uint(o, 256)
    assert_equal [0x19, 0x01, 0x00], o
  end

  def test_shortest_form_uint_65536
    o = []
    C.encode_uint(o, 65_536)
    assert_equal [0x1A, 0x00, 0x01, 0x00, 0x00], o
  end

  def test_tstr_round_trip_head
    o = []
    C.encode_tstr(o, "hello")
    # major 3 (text string), len 5 short form: 0x65, then "hello"
    assert_equal [0x65, *"hello".bytes], o
  end

  def test_bstr_round_trip_head
    o = []
    C.encode_bstr(o, "hi".b)
    # major 2 (byte string), len 2 short form: 0x42, then "hi"
    assert_equal [0x42, *"hi".bytes], o
  end

  def test_array_head
    o = []
    C.encode_array_head(o, 3)
    # major 4 (array), count 3 short form: 0x83
    assert_equal [0x83], o
  end

  def test_map_head_seven_keys
    o = []
    C.encode_map_head(o, 7)
    # major 5 (map), count 7 short form: 0xA7
    assert_equal [0xA7], o
  end
end
