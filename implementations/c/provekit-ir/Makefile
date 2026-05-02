# SPDX-License-Identifier: Apache-2.0

CC = cc
CFLAGS = -Wall -Wextra -std=c11 -Iinclude -g
LDFLAGS =

SRC = src/ir.c src/jcs.c src/hash.c
TEST_SRC = tests/test_runner.c
OBJS = $(SRC:.c=.o)

.PHONY: all test clean

all: test

test: tests/test_runner
	./tests/test_runner

tests/test_runner: $(OBJS) $(TEST_SRC)
	$(CC) $(CFLAGS) -o $@ $(OBJS) $(TEST_SRC) $(LDFLAGS)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(OBJS) tests/test_runner
