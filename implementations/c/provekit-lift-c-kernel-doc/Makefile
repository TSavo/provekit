# SPDX-License-Identifier: Apache-2.0

CC = cc
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -I../provekit-lift-core/include -g
LLVM_CONFIG ?= $(shell command -v llvm-config 2>/dev/null || command -v /usr/local/opt/llvm/bin/llvm-config 2>/dev/null || command -v /usr/local/opt/llvm@21/bin/llvm-config 2>/dev/null)

BIN = provekit-lift-c-kernel-doc
SRC = src/main.c src/kernel_doc.c ../provekit-lift-core/src/core.c ../provekit-lift-core/src/parser.c ../provekit-lift-core/src/compile_context.c
TEST_SH = tests/integration.sh

ifneq ($(strip $(LLVM_CONFIG)),)
CFLAGS += -DPK_C_ENABLE_CLANG_AST -I$(shell $(LLVM_CONFIG) --includedir)
LDFLAGS += -L$(shell $(LLVM_CONFIG) --libdir) -Wl,-rpath,$(shell $(LLVM_CONFIG) --libdir) -lclang
SRC += ../provekit-lift-core/src/clang_ast.c
else
SRC += ../provekit-lift-core/src/clang_ast_stub.c
endif

.PHONY: all test clean

all: $(BIN)

$(BIN): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC) $(LDFLAGS)

test: $(BIN) $(TEST_SH)
	@chmod +x $(TEST_SH)
	@$(TEST_SH)

clean:
	rm -f $(BIN)
