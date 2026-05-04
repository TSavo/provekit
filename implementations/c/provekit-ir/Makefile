# SPDX-License-Identifier: Apache-2.0
#
# C kit build. BLAKE3 is vendored at <repo>/tools/blake3-vendored and
# compiled with all SIMD paths disabled, so there is no host BLAKE3
# install requirement and the build is hermetic on any C11 toolchain.

CC = cc
B3_DIR = ../../../tools/blake3-vendored
B3_FLAGS = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0
CFLAGS = -Wall -Wextra -std=c11 -Iinclude -I$(B3_DIR) $(B3_FLAGS) -g
LDFLAGS =

KIT_SRC = src/ir.c src/jcs.c src/hash.c
B3_SRC = $(B3_DIR)/blake3.c $(B3_DIR)/blake3_dispatch.c $(B3_DIR)/blake3_portable.c
TEST_SRC = tests/test_runner.c

KIT_OBJS = $(KIT_SRC:.c=.o)
B3_OBJS = $(B3_SRC:.c=.o)
OBJS = $(KIT_OBJS) $(B3_OBJS)

.PHONY: all test clean

all: test

test: tests/test_runner
	./tests/test_runner

tests/test_runner: $(OBJS) $(TEST_SRC)
	$(CC) $(CFLAGS) -o $@ $(OBJS) $(TEST_SRC) $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(KIT_OBJS) $(B3_OBJS) tests/test_runner
