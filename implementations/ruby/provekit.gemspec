# SPDX-License-Identifier: Apache-2.0

Gem::Specification.new do |s|
  s.name        = "provekit"
  # 0.1.1: BLAKE3 C extension renamed from `provekit/blake3` to
  # `provekit_blake3` to avoid name collision with the lib wrapper
  # (PR #294). Bumping the version busts setup-ruby@v1's
  # bundler-cache, which keys on gemspec contents and was restoring
  # a stale compiled .so under the old name.
  s.version     = "0.1.1"
  s.licenses    = ["Apache-2.0"]
  s.summary     = "ProvekIt Ruby kit: native crypto substrate (BLAKE3 + JCS + Ed25519 + CBOR)."
  s.description = <<~DESC
    Native Ruby implementation of the ProvekIt substrate primitives:
    BLAKE3-512 hashing (vendored C extension, zero system deps), JCS canonical
    JSON, Ed25519 sign/verify, deterministic CBOR (RFC 8949 §4.2.1), and .proof
    envelope construction. Output is byte-identical to the Rust + Python kits.
  DESC
  s.authors     = ["ProvekIt contributors"]
  s.homepage    = "https://github.com/TSavo/provekit"
  s.metadata    = {
    "source_code_uri" => "https://github.com/TSavo/provekit",
    "issue_tracker_uri" => "https://github.com/TSavo/provekit/issues",
  }

  s.required_ruby_version = ">= 3.0"

  s.files = Dir["lib/**/*.rb"] + Dir["ext/**/*.{c,h,rb}"] + Dir["bin/*"] + ["README.md", "manifest.json"]
  s.extensions = ["ext/provekit_blake3/extconf.rb"]
  s.require_paths = ["lib"]

  s.add_dependency "ed25519", "~> 1.4"
  s.add_dependency "base64", ">= 0.2"

  s.add_development_dependency "minitest", ">= 5.0"
  s.add_development_dependency "rake", ">= 13.0"
end
