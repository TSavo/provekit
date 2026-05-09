# SPDX-License-Identifier: Apache-2.0

CC = cc
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -I../provekit-lift-core/include -g
LLVM_CONFIG ?= $(shell command -v llvm-config 2>/dev/null || command -v /usr/local/opt/llvm/bin/llvm-config 2>/dev/null || command -v /usr/local/opt/llvm@21/bin/llvm-config 2>/dev/null)

UNAME_S := $(shell uname -s)

BIN = provekit-lift-c-kunit
SRC = src/main.c src/kunit.c ../provekit-lift-core/src/core.c ../provekit-lift-core/src/parser.c ../provekit-lift-core/src/compile_context.c
TEST_SH = tests/integration.sh

# Path to libprovekit's C ABI staticlib + header (CCP §6.2). Built via
# `cargo build --release -p libprovekit` from implementations/rust.
LIBPROVEKIT_INCLUDE := ../../rust/libprovekit/include
LIBPROVEKIT_STATICLIB := ../../rust/target/release/liblibprovekit.a

ifneq ($(strip $(LLVM_CONFIG)),)
CFLAGS += -DPK_C_ENABLE_CLANG_AST -I$(shell $(LLVM_CONFIG) --includedir)
LDFLAGS += -L$(shell $(LLVM_CONFIG) --libdir) -Wl,-rpath,$(shell $(LLVM_CONFIG) --libdir) -lclang
SRC += ../provekit-lift-core/src/clang_ast.c ../provekit-lift-core/src/effects.c
else
SRC += ../provekit-lift-core/src/clang_ast_stub.c
endif

.PHONY: all test clean libprovekit

all: $(BIN)

# Build libprovekit's staticlib (lazy): only invoked when the artifact
# is missing. Re-running cargo on every build is too expensive for the
# inner loop; deliberate `make libprovekit` is the way to refresh.
$(LIBPROVEKIT_STATICLIB):
	cd ../../rust && cargo build --release -p libprovekit

libprovekit:
	cd ../../rust && cargo build --release -p libprovekit

ifneq ($(strip $(LLVM_CONFIG)),)
$(BIN): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC) $(LDFLAGS)
else
$(BIN): $(SRC)
	$(CC) $(CFLAGS) -o $@ $(SRC) $(LDFLAGS)
endif

test: $(BIN) $(TEST_SH)
	@chmod +x $(TEST_SH)
	@$(TEST_SH)

clean:
	rm -f $(BIN)
