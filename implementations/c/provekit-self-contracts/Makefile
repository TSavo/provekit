# SPDX-License-Identifier: Apache-2.0
#
# provekit-self-contracts (C kit) — Side B build.
#
# Native crypto library: JCS (RFC 8785) + CBOR (RFC 8949 §4.2.1) +
# Ed25519 (libsodium) + BLAKE3-512 (vendored, SIMD-disabled) + proof
# envelope builder/verifier.
#
# System dep: libsodium. macOS: `brew install libsodium`. Debian/Ubuntu:
# `apt-get install -y libsodium-dev`. CI installs libsodium-dev in
# `.github/workflows/ci.yml` alongside the existing C++ system deps.
#
# Hermetic on a C11 toolchain + libsodium. BLAKE3 is vendored at
# tools/blake3-vendored and compiled with all SIMD paths disabled.

CC      = cc
B3_DIR  = ../../../tools/blake3-vendored
B3_FLAGS = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0

# libsodium include/lib path resolution: try Homebrew prefixes, then
# system. CI on Linux self-hosted has libsodium under /usr/include.
SODIUM_PREFIX_HOMEBREW_ARM = /opt/homebrew
SODIUM_PREFIX_HOMEBREW_X86 = /usr/local
SODIUM_PREFIX ?= $(shell \
    if [ -f $(SODIUM_PREFIX_HOMEBREW_ARM)/include/sodium.h ]; then echo $(SODIUM_PREFIX_HOMEBREW_ARM); \
    elif [ -f $(SODIUM_PREFIX_HOMEBREW_X86)/include/sodium.h ]; then echo $(SODIUM_PREFIX_HOMEBREW_X86); \
    else echo /usr; fi)

SODIUM_INCLUDE = -I$(SODIUM_PREFIX)/include
SODIUM_LDFLAGS = -L$(SODIUM_PREFIX)/lib -lsodium

CFLAGS  = -Wall -Wextra -std=c11 -Iinclude -I$(B3_DIR) $(SODIUM_INCLUDE) $(B3_FLAGS) -g
LDFLAGS = $(SODIUM_LDFLAGS)

KIT_SRC  = src/bytes.c src/cbor.c src/jcs.c src/sign.c src/hash.c src/proof_envelope.c
B3_SRC   = $(B3_DIR)/blake3.c $(B3_DIR)/blake3_dispatch.c $(B3_DIR)/blake3_portable.c
TEST_SRC = tests/test_runner.c

KIT_OBJS = $(KIT_SRC:.c=.o)
B3_OBJS  = $(B3_SRC:.c=.o)
OBJS     = $(KIT_OBJS) $(B3_OBJS)

LIBNAME = libprovekit-self-contracts.a

.PHONY: all test lib clean

all: test

lib: $(LIBNAME)

$(LIBNAME): $(OBJS)
	ar rcs $@ $(OBJS)

test: tests/test_runner
	./tests/test_runner

tests/test_runner: $(OBJS) $(TEST_SRC)
	$(CC) $(CFLAGS) -o $@ $(OBJS) $(TEST_SRC) $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(KIT_OBJS) $(B3_OBJS) tests/test_runner $(LIBNAME)
