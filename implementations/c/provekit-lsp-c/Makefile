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
CFLAGS  = -std=c11 -Wall -Wextra -Wpedantic -O2 -I../provekit-lift-core/include

BIN     = provekit-lsp-c
SRC     = main.c ../provekit-lift-core/src/core.c ../provekit-lift-core/src/parser.c
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
