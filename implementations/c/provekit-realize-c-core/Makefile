# SPDX-License-Identifier: Apache-2.0

CC ?= cc
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -O2 -g

SRC = src/main.c
BIN = target/release/provekit-realize-c
DISCOVERY_BIN = ../target/release/provekit-realize-c
TEST_SH = tests/integration.sh

.PHONY: all clean test

all: $(BIN)

$(BIN): $(SRC)
	mkdir -p target/release ../target/release
	$(CC) $(CFLAGS) -o $@ $(SRC)
	cp $@ $(DISCOVERY_BIN)

test: $(BIN) $(TEST_SH)
	@chmod +x $(TEST_SH)
	@$(TEST_SH)

clean:
	rm -rf target ../target/release/provekit-realize-c
