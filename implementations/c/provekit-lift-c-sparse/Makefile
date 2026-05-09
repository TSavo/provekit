# SPDX-License-Identifier: Apache-2.0

CC = cc
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -I../provekit-lift-core/include -g

BIN = provekit-lift-c-sparse
SRC = src/main.c src/sparse.c ../provekit-lift-core/src/core.c ../provekit-lift-core/src/parser.c
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
