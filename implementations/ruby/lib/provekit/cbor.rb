# SPDX-License-Identifier: Apache-2.0
#
# Deterministic CBOR encoder. RFC 8949 §4.2.1 rules:
#   - shortest-form integer encoding (smallest of short / u8 / u16 / u32 / u64)
#   - definite-length items only
#   - map keys sorted in bytewise lex order of their CBOR-encoded form
#   - we emit only the major types we need: unsigned int, byte string,
#     text string, array, map.
#
# Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs 1:1.

module Provekit
  module Cbor
    # CBOR major type values (RFC 8949 §3.1).
    UNSIGNED_INT = 0
    BYTE_STRING  = 2
    TEXT_STRING  = 3
    ARRAY        = 4
    MAP          = 5

    # Append the CBOR head (major type + argument) to +out+.
    # Selects the shortest-form encoding per RFC 8949 §4.2.1.
    def self.append_head(out, major, arg)
      mt = major << 5
      if arg < 24
        out << (mt | arg)
        return
      end
      if arg <= 0xFF
        out << (mt | 24)
        out << arg
        return
      end
      if arg <= 0xFFFF
        out << (mt | 25)
        out << ((arg >> 8) & 0xFF)
        out << (arg & 0xFF)
        return
      end
      if arg <= 0xFFFF_FFFF
        out << (mt | 26)
        out << ((arg >> 24) & 0xFF)
        out << ((arg >> 16) & 0xFF)
        out << ((arg >> 8) & 0xFF)
        out << (arg & 0xFF)
        return
      end
      out << (mt | 27)
      7.downto(0) { |i| out << ((arg >> (i * 8)) & 0xFF) }
    end

    def self.encode_uint(out, value)
      append_head(out, UNSIGNED_INT, value)
    end

    def self.encode_bstr(out, bytes)
      raw = bytes.is_a?(String) ? bytes.b : bytes.to_s.b
      append_head(out, BYTE_STRING, raw.bytesize)
      raw.each_byte { |b| out << b }
    end

    def self.encode_tstr(out, str)
      raw = str.to_s.dup.force_encoding(Encoding::UTF_8)
      bytes = raw.bytes
      append_head(out, TEXT_STRING, bytes.length)
      bytes.each { |b| out << b }
    end

    def self.encode_array_head(out, count)
      append_head(out, ARRAY, count)
    end

    def self.encode_map_head(out, count)
      append_head(out, MAP, count)
    end

    # Encode a key string for use as a map key.
    # Returns an Array<Integer> of bytes.
    def self.encode_key(key)
      buf = []
      encode_tstr(buf, key)
      buf
    end

    # Pack an array of byte integers into a binary String.
    def self.pack_bytes(byte_array)
      byte_array.pack("C*")
    end
  end
end
