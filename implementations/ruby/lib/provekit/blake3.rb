# SPDX-License-Identifier: Apache-2.0
#
# BLAKE3-512 hash for ProvekIt (64-byte XOF output).
#
# Backed natively by libblake3 via FFI. No subprocess shell-out.
#
# Prereq: libblake3 must be installed on the system.
#   macOS:  brew install blake3
#   Debian: apt-get install libblake3-dev
#   Other:  https://github.com/BLAKE3-team/BLAKE3 (build from source)
#
# Usage:
#   require "provekit/blake3"
#   Provekit::Blake3.hex("data")    # => "blake3-512:486a4c..."
#   Provekit::Blake3.bytes("data")  # => raw 64-byte String
#
# Verified byte-identical to:
#   - Rust: implementations/rust/provekit-canonicalizer (blake3_512_of)
#   - Python: blake3 PyPI package, .digest(length=64)

require "ffi"

module Provekit
  module Blake3
    # libblake3 binding. Search common platform paths in order; first hit wins.
    # macOS Homebrew (intel + arm), Debian/Ubuntu, generic SONAME.
    module Native
      extend FFI::Library

      LIB_CANDIDATES = [
        "blake3",
        "libblake3.so.1",
        "libblake3.so.0",
        "libblake3.so",
        "libblake3.dylib",
        "libblake3.0.dylib",
        "/usr/local/lib/libblake3.dylib",   # macOS x86_64 Homebrew
        "/opt/homebrew/lib/libblake3.dylib", # macOS arm64 Homebrew
        "/usr/lib/libblake3.so.1",
        "/usr/lib/x86_64-linux-gnu/libblake3.so.1",
        "/usr/lib/aarch64-linux-gnu/libblake3.so.1",
      ].freeze

      ffi_lib LIB_CANDIDATES

      # blake3_hasher state struct sized generously per blake3.h:
      #   key (32) + chunk_state (~84) + cv_stack_len (1) + cv_stack ((54+1)*32 = 1760)
      #   ≈ 1880 bytes; we round up to 4 KiB for safety across libblake3 minor versions.
      HASHER_SIZE = 4096

      attach_function :blake3_hasher_init,     [:pointer],                          :void
      attach_function :blake3_hasher_update,   [:pointer, :pointer, :size_t],       :void
      attach_function :blake3_hasher_finalize, [:pointer, :pointer, :size_t],       :void
    end

    # Output BLAKE3-512 in the protocol's self-identifying string form:
    #   "blake3-512:" + 128 lowercase hex chars.
    # +data+ is treated as binary bytes.
    def self.hex(data)
      "blake3-512:" + bytes(data).unpack1("H*")
    end

    # Compute raw 64-byte BLAKE3-512 of +data+. Returns a binary String of
    # length 64 (ASCII-8BIT encoding).
    def self.bytes(data)
      raw = data.is_a?(String) ? data.b : data.to_s.b
      hasher = FFI::MemoryPointer.new(:uint8, Native::HASHER_SIZE)
      Native.blake3_hasher_init(hasher)
      unless raw.bytesize.zero?
        in_buf = FFI::MemoryPointer.new(:uint8, raw.bytesize)
        in_buf.put_bytes(0, raw)
        Native.blake3_hasher_update(hasher, in_buf, raw.bytesize)
      end
      out_buf = FFI::MemoryPointer.new(:uint8, 64)
      Native.blake3_hasher_finalize(hasher, out_buf, 64)
      out_buf.read_bytes(64)
    end
  end
end
