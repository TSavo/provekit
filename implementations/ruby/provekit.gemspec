# SPDX-License-Identifier: Apache-2.0

Gem::Specification.new do |s|
  s.name        = "provekit"
  s.version     = "0.1.0"
  s.licenses    = ["Apache-2.0"]
  s.summary     = "ProvekIt Ruby kit: native crypto substrate (BLAKE3 + JCS + Ed25519 + CBOR)."
  s.description = <<~DESC
    Native Ruby implementation of the ProvekIt substrate primitives:
    BLAKE3-512 hashing, JCS canonical JSON, Ed25519 sign/verify, deterministic
    CBOR (RFC 8949 §4.2.1), and .proof envelope construction. Output is
    byte-identical to the Rust + Python kits' output for the same input.

    Prereq: libblake3 must be installed on the host system.
      macOS:  brew install blake3
      Debian: apt-get install libblake3-dev
  DESC
  s.authors     = ["ProvekIt contributors"]
  s.homepage    = "https://github.com/TSavo/provekit"
  s.metadata    = {
    "source_code_uri" => "https://github.com/TSavo/provekit",
    "issue_tracker_uri" => "https://github.com/TSavo/provekit/issues",
  }

  s.required_ruby_version = ">= 3.0"

  s.files = Dir["lib/**/*.rb"]
  s.require_paths = ["lib"]

  s.add_dependency "ed25519", "~> 1.4"
  s.add_dependency "ffi",     "~> 1.15"

  s.add_development_dependency "minitest", ">= 5.0"
end
