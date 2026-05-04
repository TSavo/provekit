# SPDX-License-Identifier: Apache-2.0
#
# C kit build. BLAKE3 is vendored at <repo>/tools/blake3-vendored and
# compiled with all SIMD paths disabled, so there is no host BLAKE3
# install requirement and the build is hermetic on any C11 toolchain.
#
# Ed25519 uses OpenSSL libcrypto (EVP_PKEY_ED25519). The thin wrapper at
# tools/ed25519-vendored/ed25519.c isolates the dependency; build flag:
#   -DPROVEKIT_ED25519_OPENSSL   (always set; documents the backend)
#
# Requires OpenSSL 1.1+ or 3.x headers and libcrypto at link time.
# On macOS with Homebrew: brew install openssl@3 ; set OPENSSL_PREFIX below.
#
# Override on the command line:
#   make OPENSSL_PREFIX=/usr/local

CC          = cc
OPENSSL_PREFIX ?= /usr/local

B3_DIR      = ../../../tools/blake3-vendored
ED_DIR      = ../../../tools/ed25519-vendored
B3_FLAGS    = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0
CFLAGS      = -Wall -Wextra -std=c11 -Iinclude -I$(B3_DIR) -I$(ED_DIR) \
              -I$(OPENSSL_PREFIX)/include \
              $(B3_FLAGS) -DPROVEKIT_ED25519_OPENSSL -g
LDFLAGS     = -L$(OPENSSL_PREFIX)/lib -lcrypto

KIT_SRC     = src/ir.c src/jcs.c src/hash.c \
              src/value.c src/sign.c src/cbor.c \
              src/claim_envelope.c src/proof_envelope.c
ED_SRC      = $(ED_DIR)/ed25519.c
B3_SRC      = $(B3_DIR)/blake3.c $(B3_DIR)/blake3_dispatch.c $(B3_DIR)/blake3_portable.c
TEST_SRC    = tests/test_runner.c

KIT_OBJS    = $(KIT_SRC:.c=.o)
ED_OBJS     = $(ED_DIR)/ed25519.o
B3_OBJS     = $(B3_SRC:.c=.o)
OBJS        = $(KIT_OBJS) $(ED_OBJS) $(B3_OBJS)

.PHONY: all test clean

all: test

test: tests/test_runner
	./tests/test_runner

tests/test_runner: $(OBJS) $(TEST_SRC)
	$(CC) $(CFLAGS) -o $@ $(OBJS) $(TEST_SRC) $(LDFLAGS)

$(ED_DIR)/ed25519.o: $(ED_DIR)/ed25519.c
	$(CC) $(CFLAGS) -c $< -o $@

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(KIT_OBJS) $(B3_OBJS) $(ED_OBJS) tests/test_runner
