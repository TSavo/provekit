/* SPDX-License-Identifier: Apache-2.0 */
/*
 * provekit-ir — C kit for ProvekIt protocol v1.1.0.
 *
 * Mirrors the Rust/Go/Java/Python kits. All IR nodes are tagged unions
 * allocated on the heap; caller frees with pk_*_free functions.
 */

#ifndef PROVEKIT_IR_H
#define PROVEKIT_IR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ----------------------------------------------------------------------- */
/* Sort                                                                    */
/* ----------------------------------------------------------------------- */

typedef enum {
    PK_SORT_PRIMITIVE,
    PK_SORT_FUNCTION,
    PK_SORT_DEPENDENT,
    PK_SORT_REGION,
} pk_sort_kind;

typedef struct pk_sort pk_sort;

struct pk_sort {
    pk_sort_kind kind;
    union {
        struct { char *name; } primitive;
        struct { pk_sort **args; size_t n_args; pk_sort *ret; } function;
        struct { char *name; char *index_var; pk_sort *index_sort; } dependent;
        struct { char *name; } region;
    } data;
};

pk_sort *pk_sort_primitive(const char *name);
pk_sort *pk_sort_function(pk_sort **args, size_t n_args, pk_sort *ret);
pk_sort *pk_sort_dependent(const char *name, const char *index_var, pk_sort *index_sort);
pk_sort *pk_sort_region(const char *name);
void pk_sort_free(pk_sort *s);

/* ----------------------------------------------------------------------- */
/* Term                                                                    */
/* ----------------------------------------------------------------------- */

typedef enum {
    PK_TERM_VAR,
    PK_TERM_CONST,
    PK_TERM_CTOR,
} pk_term_kind;

typedef struct pk_term pk_term;

typedef struct {
    char *name; /* owned */
} pk_term_var;

typedef struct {
    void *value;       /* owned: int64_t* | char* | int* */
    pk_sort *sort;     /* owned */
    int is_string;     /* 1 => value is char*, 0 => int64_t*, 2 => int* bool, 3 => null */
} pk_term_const;

typedef struct {
    char *name;        /* owned */
    pk_term **args;    /* owned array of owned pointers */
    size_t n_args;
} pk_term_ctor;

struct pk_term {
    pk_term_kind kind;
    union {
        pk_term_var var;
        pk_term_const constant;
        pk_term_ctor ctor;
    } data;
};

pk_term *pk_term_var_new(const char *name);
pk_term *pk_term_const_int(int64_t value, pk_sort *sort);
pk_term *pk_term_const_str(const char *value, pk_sort *sort);
pk_term *pk_term_const_bool(int value, pk_sort *sort);
pk_term *pk_term_const_null(pk_sort *sort);
pk_term *pk_term_ctor_new(const char *name, pk_term **args, size_t n_args);
void pk_term_free(pk_term *t);

/* ----------------------------------------------------------------------- */
/* Formula                                                                 */
/* ----------------------------------------------------------------------- */

typedef enum {
    PK_FORMULA_ATOMIC,
    PK_FORMULA_CONNECTIVE,
    PK_FORMULA_QUANTIFIER,
} pk_formula_kind;

typedef struct pk_formula pk_formula;

typedef struct {
    char *name;        /* owned */
    pk_term **args;    /* owned array of owned pointers */
    size_t n_args;
} pk_formula_atomic;

typedef struct {
    char *kind;        /* owned: "and", "or", "not", "implies" */
    pk_formula **operands; /* owned array of owned pointers */
    size_t n_operands;
} pk_formula_connective;

typedef struct {
    char *kind;        /* owned: "forall", "exists" */
    char *name;        /* owned */
    pk_sort *sort;     /* owned */
    pk_formula *body;  /* owned */
} pk_formula_quantifier;

struct pk_formula {
    pk_formula_kind kind;
    union {
        pk_formula_atomic atomic;
        pk_formula_connective connective;
        pk_formula_quantifier quantifier;
    } data;
};

pk_formula *pk_formula_atomic_new(const char *name, pk_term **args, size_t n_args);
pk_formula *pk_formula_connective_new(const char *kind, pk_formula **operands, size_t n_operands);
pk_formula *pk_formula_quantifier_new(const char *kind, const char *name, pk_sort *sort, pk_formula *body);
void pk_formula_free(pk_formula *f);

/* ----------------------------------------------------------------------- */
/* Declaration                                                             */
/* ----------------------------------------------------------------------- */

typedef enum {
    PK_DECL_CONTRACT,
    PK_DECL_BRIDGE,
} pk_decl_kind;

typedef struct pk_decl pk_decl;

typedef struct {
    char *name;        /* owned */
    char *out_binding; /* owned */
    pk_formula *pre;   /* owned, nullable */
    pk_formula *post;  /* owned, nullable */
    pk_formula *inv;   /* owned, nullable */
} pk_decl_contract;

typedef struct {
    char *name;                /* owned */
    char *source_symbol;       /* owned */
    char *source_layer;        /* owned */
    char *source_contract_cid; /* owned */
    char *target_contract_cid; /* owned */
    char *target_proof_cid;    /* owned */
    char *target_layer;        /* owned */
    char *notes;               /* owned, nullable */
} pk_decl_bridge;

struct pk_decl {
    pk_decl_kind kind;
    union {
        pk_decl_contract contract;
        pk_decl_bridge bridge;
    } data;
};

pk_decl *pk_decl_contract_new(const char *name, const char *out_binding,
                               pk_formula *pre, pk_formula *post, pk_formula *inv);
pk_decl *pk_decl_bridge_new(const char *name, const char *source_symbol,
                            const char *source_layer, const char *source_contract_cid,
                            const char *target_contract_cid, const char *target_proof_cid,
                            const char *target_layer, const char *notes);
void pk_decl_free(pk_decl *d);

/* ----------------------------------------------------------------------- */
/* Buffer / Emitter                                                        */
/* ----------------------------------------------------------------------- */

typedef struct {
    char *data;  /* owned, null-terminated */
    size_t len;
    size_t cap;
} pk_buffer;

pk_buffer *pk_buffer_new(void);
void pk_buffer_free(pk_buffer *buf);
void pk_buffer_append(pk_buffer *buf, const char *s);
void pk_buffer_append_char(pk_buffer *buf, char c);
char *pk_buffer_steal(pk_buffer *buf); /* caller frees */

void pk_emit_sort(pk_buffer *buf, pk_sort *s);
void pk_emit_term(pk_buffer *buf, pk_term *t);
void pk_emit_formula(pk_buffer *buf, pk_formula *f);
void pk_emit_decl(pk_buffer *buf, pk_decl *d);
void pk_emit_decls(pk_buffer *buf, pk_decl **decls, size_t n_decls);

/* ----------------------------------------------------------------------- */
/* Hash (delegates to Python blake3 in v1.1; native C BLAKE3 in v1.2)      */
/* ----------------------------------------------------------------------- */

/* Returns an owned string "blake3-512:...hex..." or NULL on error. */
char *pk_hash_jcs(const char *jcs_string);

#ifdef __cplusplus
}
#endif

#endif /* PROVEKIT_IR_H */
