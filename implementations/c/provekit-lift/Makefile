# Build artifacts
*.o
bin/
*.tmp

# Test output
test_output.txt

CC = clang
CFLAGS = -Wall -Wextra -std=c11 -fno-builtin
LLVM = /usr/local/opt/llvm
PROVEKIT_INCLUDE = -I/Users/tsavo/provekit-380/implementations/c/provekit-lift/include
CLANG_INCLUDE = -I$(LLVM)/include
LDFLAGS = -L$(LLVM)/lib
LLDB = -lclang

SRC = src/lift.c
OBJ = $(SRC:.c=.o)
BIN = bin/provekit-lift-c
MAIN = -DPROVEKIT_LIFT_C_MAIN

all: $(BIN)

$(BIN): $(OBJ) | bin
	$(CC) -o $@ $^ $(LDFLAGS) $(LLDB)

%.o: %.c include/provekit/*.h
	$(CC) $(CFLAGS) $(PROVEKIT_INCLUDE) $(CLANG_INCLUDE) $(MAIN) -c $< -o $@

bin:
	mkdir -p bin

clean:
	rm -f $(OBJ) $(BIN)

test: $(BIN)
	./$(BIN) tests/test_input.c