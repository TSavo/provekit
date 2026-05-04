# SPDX-License-Identifier: Apache-2.0
#
# BLAKE3-512 hash for ProvekIt (64-byte XOF output).
#
# Backed by a vendored C extension that statically links
# tools/blake3-vendored/. Zero system deps — no libblake3,
# no FFI, no subprocess.
#
# Usage:
#   require "provekit/blake3"
#   Provekit::Blake3.hex("data")    # => "blake3-512:486a4c..."
#   Provekit::Blake3.bytes("data")  # => raw 64-byte String
#
# Verified byte-identical to:
#   - Rust: implementations/rust/provekit-canonicalizer (blake3_512_of)
#   - Python: blake3 PyPI package, .digest(length=64)

require "provekit/blake3" rescue begin
  # Native extension not yet compiled — try require directly
  require_relative "../../ext/provekit_blake3/provekit_blake3"
end

module Provekit
  class Blake3
    HASHER_SIZE = 4096

    # Output BLAKE3-512 in the protocol's self-identifying string form:
    #   "blake3-512:" + 128 lowercase hex chars.
    def self.hex(data)
      "blake3-512:" + bytes(data).unpack1("H*")
    end

    # Compute raw 64-byte BLAKE3-512 of +data+. Returns a binary String.
    def self.bytes(data)
      raw = data.is_a?(String) ? data.b : data.to_s.b
      hasher = Provekit::Blake3.hasher_init
      Provekit::Blake3.hasher_update(hasher, raw) unless raw.bytesize.zero?
      Provekit::Blake3.hasher_finalize(hasher, 64)
    end
  end
end
