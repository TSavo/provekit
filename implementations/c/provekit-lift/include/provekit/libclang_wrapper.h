/* Wrapper to include libclang headers FIRST, before system headers */

#ifndef LIBCLANG_WRAPPER_H
#define LIBCLANG_WRAPPER_H

/* Force these first - before any system headers pollute the namespace */
#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

/* Now libclang headers should work */
#include <clang-c/Index.h>

#endif /* LIBCLANG_WRAPPER_H */