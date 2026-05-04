# extconf.rb for provekit_blake3 C extension.
# Statically links vendored BLAKE3 from tools/blake3-vendored/.
# Portable-only, zero system deps.

require "mkmf"
require "fileutils"

dir_config("blake3")

$CFLAGS << " -std=c11"
$DEFLIBPATH = []

# Vendored BLAKE3 root.
#
# Layout: __dir__ = implementations/ruby/ext/provekit_blake3 (4 levels deep).
# Repo root = __dir__/../../../.. = 4 levels up. tools/blake3-vendored is
# under repo root. So the relative path is "../../../../tools/blake3-vendored"
# (NOT "../../../../../" which goes one too many).
B3_DIR = File.expand_path("../../../../tools/blake3-vendored", __dir__)
abort "vendored BLAKE3 not found at #{B3_DIR}" unless File.exist?(File.join(B3_DIR, "blake3.c"))

# Copy vendored sources + headers into the ext dir IF they're not already
# co-located. When the gem is built (`gem build`), gemspec ships the copied
# files in the gem tarball under ext/, so a downstream `gem install` finds
# them locally without needing tools/blake3-vendored at install time.
%w[blake3.c blake3_portable.c blake3_dispatch.c blake3.h blake3_impl.h].each do |fn|
  src = File.join(B3_DIR, fn)
  dst = File.join(__dir__, fn)
  next unless File.exist?(src)  # gem-install context: src may not exist
  FileUtils.cp(src, dst) unless File.exist?(dst) && File.mtime(dst) >= File.mtime(src)
end

# Compile vendored .c files as part of the extension
$objs = %w[provekit_blake3.o blake3.o blake3_portable.o blake3_dispatch.o]

# Configure vendored BLAKE3 for portable build
$CFLAGS << " -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41"

# IMPORTANT: name MUST be `provekit_blake3` NOT `provekit/blake3`. The
# pure-Ruby wrapper at lib/provekit/blake3.rb takes the `provekit/blake3`
# logical name; the .so needs a distinct name so `require "provekit_blake3"`
# from the wrapper loads the .so (not itself).
create_makefile("provekit_blake3")
