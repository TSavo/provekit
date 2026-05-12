# SPDX-License-Identifier: Apache-2.0
#
# mint-c-self-contracts (Side A orchestrator).
#
# Walks the C kit's slabs (src/c_kit_invariants.c) and emits a signed
# proof-envelope catalog via the Side B static library
# (libprovekit-self-contracts.a). Two modes:
#
#   ./mint-c-self-contracts [outDir]    standalone CLI
#   ./mint-c-self-contracts --rpc       pep/1.7.0 daemon over stdio
#
# System dep: libsodium (transitively, via Side B). Hermetic on a C11
# toolchain + libsodium. BLAKE3 is vendored under tools/blake3-vendored.

CC      = cc

# Mirror Side B's libsodium prefix detection so the orchestrator builds
# in the same environments without manual config.
SIDEB_DIR = ../provekit-self-contracts
B3_DIR    = ../../../tools/blake3-vendored
B3_FLAGS  = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0

SODIUM_PREFIX_HOMEBREW_ARM = /opt/homebrew
SODIUM_PREFIX_HOMEBREW_X86 = /usr/local
SODIUM_PREFIX ?= $(shell \
    if [ -f $(SODIUM_PREFIX_HOMEBREW_ARM)/include/sodium.h ]; then echo $(SODIUM_PREFIX_HOMEBREW_ARM); \
    elif [ -f $(SODIUM_PREFIX_HOMEBREW_X86)/include/sodium.h ]; then echo $(SODIUM_PREFIX_HOMEBREW_X86); \
    else echo /usr; fi)

SODIUM_INCLUDE = -I$(SODIUM_PREFIX)/include
SODIUM_LDFLAGS = -L$(SODIUM_PREFIX)/lib -lsodium

CFLAGS  = -Wall -Wextra -std=c11 \
          -Isrc \
          -I$(SIDEB_DIR)/include \
          -I$(B3_DIR) \
          $(SODIUM_INCLUDE) \
          $(B3_FLAGS) \
          -g
LDFLAGS = $(SODIUM_LDFLAGS)

LIB_SIDEB = $(SIDEB_DIR)/libprovekit-self-contracts.a

SRC = src/slab.c src/c_kit_invariants.c src/orchestrator.c src/rpc.c src/main.c
OBJ = $(SRC:.c=.o)

BIN = mint-c-self-contracts

.PHONY: all clean lib-sideb

all: $(BIN)

# Ensure the Side B static library exists. We delegate to its Makefile.
lib-sideb:
	$(MAKE) -C $(SIDEB_DIR) lib

$(LIB_SIDEB):
	$(MAKE) -C $(SIDEB_DIR) lib

$(BIN): $(OBJ) $(LIB_SIDEB)
	$(CC) $(CFLAGS) -o $@ $(OBJ) $(LIB_SIDEB) $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(OBJ) $(BIN)
