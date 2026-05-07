# SPDX-License-Identifier: Apache-2.0
#
# CICP language-kit conformance for protocol/conformance/cicp golden vectors.

require "json"
require "minitest/autorun"
require_relative "../lib/provekit/blake3"
require_relative "../lib/provekit/ir"

class TestCicpConformance < Minitest::Test
  ROOT = File.expand_path("../../..", __dir__)
  CICP_DIR = File.join(ROOT, "protocol", "conformance", "cicp")
  VECTORS = JSON.parse(File.read(File.join(CICP_DIR, "vectors.json")))

  def test_passing_vectors_match_blake3_512_of_jcs_body
    passing_vectors.each do |vector|
      body = read_body(vector)
      canonical = Provekit::IR::Jcs.encode(body)

      assert_equal(
        vector.fetch("expectedCid"),
        Provekit::Blake3.hex(canonical),
        "#{vector.fetch("name")} CID must match BLAKE3-512(JCS(body))",
      )
    end
  end

  def test_invalid_blast_radius_fails_closed_on_missing_input_cids_dependency
    vector = VECTORS.fetch("vectors").find { |v| v.fetch("shouldPass") == false }
    error = assert_raises(ArgumentError) { validate_cicp_body(read_body(vector)) }

    assert_includes error.message, vector.fetch("errorContains")
  end

  private

  def passing_vectors
    VECTORS.fetch("vectors").select { |vector| vector.fetch("shouldPass") }
  end

  def read_body(vector)
    JSON.parse(File.read(File.join(CICP_DIR, vector.fetch("body"))))
  end

  def validate_cicp_body(body)
    kind = body.fetch("kind")
    if kind == "CIBlastRadius"
      validate_blast_radius_input_closure(body)
    else
      true
    end
  end

  def validate_blast_radius_input_closure(body)
    input_cids = body.fetch("inputCids")
    required_cids = [
      body.fetch("protocolCatalogCid"),
      body.fetch("jobDefinitionCid"),
      body.fetch("commandCid"),
      body.fetch("runnerIdentityCid"),
      body.fetch("sourceClosureCid"),
      *body.fetch("toolchainCids"),
      *body.fetch("lockfileCids"),
      *body.fetch("generatedInputCids"),
      *body.fetch("fixtureCids"),
      *body.fetch("relevantSpecCids"),
    ]

    missing = required_cids.find { |cid| !input_cids.include?(cid) }
    raise ArgumentError, "inputCids missing required CID: #{missing}" if missing

    true
  end
end
