#ifndef PROVEKIT_FORWARD_PROPAGATOR_H
#define PROVEKIT_FORWARD_PROPAGATOR_H

#include <stdbool.h>
#include <stddef.h>

typedef struct {
    const char* constraints[32];
    size_t constraint_count;
    bool is_top;
} Post;

typedef struct {
    const char* code;
    const char* message;
} DiagnosticResult;

void fp_add_to_catalog(const char* callee_id, Post pre, Post post);
DiagnosticResult* fp_check_callsite(const char* callee_id, Post current_post);

#endif