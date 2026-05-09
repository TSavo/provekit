# SPDX-License-Identifier: Apache-2.0

CC = cc
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -Iinclude -g
LLVM_CONFIG ?= $(shell command -v llvm-config 2>/dev/null || command -v /usr/local/opt/llvm/bin/llvm-config 2>/dev/null || command -v /usr/local/opt/llvm@21/bin/llvm-config 2>/dev/null)

SRC = src/core.c src/parser.c

ifneq ($(strip $(LLVM_CONFIG)),)
CFLAGS += -DPK_C_ENABLE_CLANG_AST -I$(shell $(LLVM_CONFIG) --includedir)
LDFLAGS += -L$(shell $(LLVM_CONFIG) --libdir) -Wl,-rpath,$(shell $(LLVM_CONFIG) --libdir) -lclang
SRC += src/clang_ast.c
else
SRC += src/clang_ast_stub.c
endif

OBJ = $(SRC:.c=.o)
TEST = tests/test_runner

.PHONY: all test clean

all: test

test: $(TEST)
	./$(TEST)

$(TEST): $(OBJ) tests/test_runner.c
	$(CC) $(CFLAGS) -o $@ $(OBJ) tests/test_runner.c $(LDFLAGS)

%.o: %.c include/provekit/c_lift_core.h
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(sort $(OBJ) src/clang_ast.o src/clang_ast_stub.o) $(TEST)
