# SPDX-License-Identifier: Apache-2.0

CC ?= cc
B3_DIR = ../../../tools/blake3-vendored
B3_FLAGS = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -O2 -g -I$(B3_DIR) $(B3_FLAGS)

SRC = src/main.c
B3_SRC = $(B3_DIR)/blake3.c $(B3_DIR)/blake3_dispatch.c $(B3_DIR)/blake3_portable.c
BIN = target/release/provekit-realize-c
DISCOVERY_BIN = $(abspath ../target/release/provekit-realize-c)
TEST_SH = tests/integration.sh

.PHONY: all clean test

all: $(BIN)

$(BIN): $(SRC)
	mkdir -p target/release $(dir $(DISCOVERY_BIN))
	$(CC) $(CFLAGS) -o $@ $(SRC) $(B3_SRC)
	cp $@ $(DISCOVERY_BIN)

test: $(BIN) $(TEST_SH)
	@chmod +x $(TEST_SH)
	@$(TEST_SH)

clean:
	rm -rf target $(DISCOVERY_BIN)
