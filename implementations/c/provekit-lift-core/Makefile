# SPDX-License-Identifier: Apache-2.0

CC = cc
CFLAGS = -Wall -Wextra -Wpedantic -std=c11 -Iinclude -g

SRC = src/core.c src/parser.c
OBJ = $(SRC:.c=.o)
TEST = tests/test_runner

.PHONY: all test clean

all: test

test: $(TEST)
	./$(TEST)

$(TEST): $(OBJ) tests/test_runner.c
	$(CC) $(CFLAGS) -o $@ $(OBJ) tests/test_runner.c

%.o: %.c include/provekit/c_lift_core.h
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(OBJ) $(TEST)
