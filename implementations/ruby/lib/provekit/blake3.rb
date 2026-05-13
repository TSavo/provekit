# SPDX-License-Identifier: Apache-2.0
#
# BLAKE3-512 hash delegate - shells out to Python blake3 module.
# Pure Ruby BLAKE3 planned for v1.2.
#
# Usage:
#   require "provekit/blake3"
#   Provekit::Blake3.hex("data")  # => "blake3-512:486a4c..."
#
# Verified against Rust canonical reference hashes.

module Provekit
  module Blake3
    def self.hex(data)
      require "tempfile"
      tmp = Tempfile.new("pk_hash")
      tmp.write(data.to_s)
      tmp.close
      hash = `python3 -c "import blake3; d=open('#{tmp.path}','rb').read(); print('blake3-512:'+blake3.blake3(d).digest(length=64).hex())"`.chomp
      tmp.unlink
      hash
    end
  end
end
