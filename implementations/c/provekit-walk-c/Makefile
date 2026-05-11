# SPDX-License-Identifier: Apache-2.0

CC = cc
B3_DIR = ../../../tools/blake3-vendored
B3_FLAGS = -DBLAKE3_NO_AVX2 -DBLAKE3_NO_AVX512 -DBLAKE3_NO_SSE2 -DBLAKE3_NO_SSE41 -DBLAKE3_USE_NEON=0
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -I../provekit-lift-core/include -I$(B3_DIR) $(B3_FLAGS) -g
LLVM_CONFIG ?= $(shell command -v llvm-config 2>/dev/null || command -v /usr/local/opt/llvm/bin/llvm-config 2>/dev/null || command -v /usr/local/opt/llvm@21/bin/llvm-config 2>/dev/null)

BIN = provekit-walk-c
TERM_BIN = provekit-c11-term-project
SRC = src/main.c src/walk.c src/wp.c src/lift.c src/contract.c src/conditional.c ../provekit-lift-core/src/core.c ../provekit-lift-core/src/parser.c ../provekit-lift-core/src/compile_context.c
B3_SRC = $(B3_DIR)/blake3.c $(B3_DIR)/blake3_dispatch.c $(B3_DIR)/blake3_portable.c
TERM_SRC = src/term_project_main.c src/wp.c src/lift.c src/c11_cursor_dispatch.generated.c $(B3_SRC)
TEST_SH = tests/integration.sh
TERM_TEST_SH = tests/term_project.sh

ifneq ($(strip $(LLVM_CONFIG)),)
CFLAGS += -DPK_C_ENABLE_CLANG_AST -I$(shell $(LLVM_CONFIG) --includedir)
LDFLAGS += -L$(shell $(LLVM_CONFIG) --libdir) -Wl,-rpath,$(shell $(LLVM_CONFIG) --libdir) -lclang
SRC += src/c11_cursor_dispatch.generated.c ../provekit-lift-core/src/clang_ast.c ../provekit-lift-core/src/effects.c
else
SRC += ../provekit-lift-core/src/clang_ast_stub.c
endif

.PHONY: all test clean

all: $(BIN) $(TERM_BIN)

$(BIN): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC) $(LDFLAGS)

$(TERM_BIN): $(TERM_SRC)
	$(CC) $(CFLAGS) -o $@ $(TERM_SRC) $(LDFLAGS)

test: $(BIN) $(TERM_BIN) $(TEST_SH) $(TERM_TEST_SH)
	@chmod +x $(TEST_SH)
	@$(TEST_SH)
	@chmod +x $(TERM_TEST_SH)
	@$(TERM_TEST_SH)

clean:
	rm -rf $(BIN) $(TERM_BIN) $(BIN).dSYM $(TERM_BIN).dSYM
