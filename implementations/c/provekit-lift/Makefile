# makefile for provekit-lift-c

CC = gcc
CFLAGS = -Wall -Wextra -std=c11 -Iinclude -I../provekit-ir/include
LDFLAGS = -lclang

SRC = src/lift.c
OBJ = $(SRC:.c=.o)
BIN = bin/provekit-lift-c

.PHONY: all clean stub test

all: $(BIN)

$(BIN): $(OBJ) | bin
	$(CC) $(CFLAGS) -o $@ $^ $(LDFLAGS)

%.o: %.c include/provekit/lift.h src/lift.h

bin:
	mkdir -p bin

clean:
	rm -f $(OBJ) $(BIN)

stub: bin
	$(CC) $(CFLAGS) -DPROVEKIT_LIFT_C_STUB -o bin/stub $(SRC)
	./bin/stub --kernel