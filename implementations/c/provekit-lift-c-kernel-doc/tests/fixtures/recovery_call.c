/* Reproduces the libclang RecoveryExpr wrapping observed in real
 * kernel C (e.g. net/ipv4/esp4.c::esp_input calling
 * aead_request_set_crypt). When a call argument has a "<dependent type>"
 * (here, the local `iv` declared with the undeclared `u8` typedef),
 * libclang wraps the entire call expression in a RecoveryExpr instead
 * of a CallExpr. Without explicit handling the lifter emits zero
 * callEdges for those calls. This fixture lets us assert recovery is
 * surfaced. */

extern void target_inplace_set();
extern void target_callback_set();
extern void target_noncall_ref();
extern void target_parenthesized_set();
extern void target_deref_set();

void caller_with_recovery(int *req, int *sg) {
    void (*fp)(void);
    u8 *iv;            /* undeclared typedef -> dependent-type lvalue */
    int elen, ivlen = 0;
    elen = 0;

    /* Function references are not calls and must not produce callEdges. */
    fp = target_noncall_ref;
    (void)target_noncall_ref;

    /* Args without a poisoned operand: regular CallExpr. */
    target_callback_set(req, 0, sg, sg);

    /* The one real call to the function-ref target must still surface. */
    target_noncall_ref();

    /* Parenthesized function designators are still direct calls. */
    (target_parenthesized_set)(req, sg, sg, elen + ivlen, iv);
    (*target_deref_set)(req, sg, sg, elen + ivlen, iv);

    /* Passing the dependent-type `iv`: libclang produces RecoveryExpr. */
    target_inplace_set(req, sg, sg, elen + ivlen, iv);
}
