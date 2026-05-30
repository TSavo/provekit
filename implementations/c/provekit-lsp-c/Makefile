# SPDX-License-Identifier: Apache-2.0
#
# provekit-lsp-c — NDJSON LSP plugin binary for C.
#
# Speaks provekit-lsp-plugin/1 over stdin/stdout.
# Wire shape matches implementations/go/cmd/provekit-lsp-go/main.go.
#
# Build: make
# Test:  make test
# Clean: make clean

CC      = cc
B3_DIR  = ../../../tools/blake3-vendored
B3_FLAGS = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0
CFLAGS  = -std=c11 -Wall -Wextra -Wpedantic -O2 -I../provekit-lift-core/include -I$(B3_DIR) $(B3_FLAGS)

BIN     = provekit-lsp-c
B3_SRC  = $(B3_DIR)/blake3.c $(B3_DIR)/blake3_dispatch.c $(B3_DIR)/blake3_portable.c
SRC     = main.c ../provekit-lift-core/src/core.c ../provekit-lift-core/src/parser.c $(B3_SRC)
TEST_SH = tests/integration.sh

.PHONY: all test clean

all: $(BIN)

$(BIN): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC)

test: $(BIN) $(TEST_SH)
	@chmod +x $(TEST_SH)
	@$(TEST_SH)

clean:
	rm -f $(BIN)
