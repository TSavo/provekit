/* Regression / scope fixture: a function's computed pre may reference only
 * state reachable and controllable at the function's own entry --
 *   - its formal parameters,
 *   - file-scope / global symbols (incl. static-at-file-scope),
 *   - state reachable from those (p->field, p[i], *p, &p->field),
 *   - compile-time constants (sizeof T, ...) and known null/bool constants.
 * Anything else -- a variable local to the function, a callee-internal name,
 * or a name we cannot classify -- is dropped from pre and surfaced as a
 * c-assertions.non-entry-state opacity entry instead (sound: a dropped clause
 * becomes honest opacity, never a silently-weakened pre).
 *
 * Note: bare assert(...) is used deliberately (no #include <assert.h>);
 * the lift harness tolerates the implicit-function-declaration warning.
 *
 * Expected lifter behaviour:
 *   - g(int y):        assert(y > 0)        -> KEPT  (formal)
 *                      assert(local_of_g>0) -> DROPPED (g's own local);
 *                      local_of_g must not appear in g's pre (or f's).
 *   - f(int x):        assert(x != 0)       -> KEPT  (formal)
 *   - h(int n):        assert(tmp > 0)      -> DROPPED (local-only) ->
 *                      h emits no hard contract; opacity emitted instead.
 *   - uses_global:     assert(file_scope_global > 0) -> KEPT (file-scope global)
 *   - member_pre:      assert(p->len > 0)   -> KEPT  (p is a formal)
 *   - index_pre:       assert(arr[0] != 0)  -> KEPT  (arr is a formal)
 *                      assert(arr[i] >= 0)  -> KEPT  (arr, i both formals)
 *   - member_index_local:
 *                      assert(p->arr[local_idx] != 0) -> DROPPED
 *                      (p is a formal but local_idx is a local); local_idx
 *                      must not appear in any pre.
 *   - shadowed_formal: a block-scope local "w" shadows the formal "w"; with no
 *                      per-locus scope the conservative call is to treat any
 *                      assert(w ...) as non-entry-state -> DROPPED, no contract.
 */

struct S {
    int len;
    int *arr;
};

static int file_scope_global = 7;

int g(int y)
{
    int local_of_g = y + 1;
    assert(y > 0);
    assert(local_of_g > 0);
    return local_of_g;
}

int f(int x)
{
    assert(x != 0);
    return g(x);
}

int h(int n)
{
    int tmp = n * 2;
    assert(tmp > 0);
    return tmp;
}

int uses_global(int v)
{
    assert(file_scope_global > 0);
    return v + file_scope_global;
}

int member_pre(struct S *p)
{
    assert(p->len > 0);
    return p->len;
}

int index_pre(int *arr, int i)
{
    assert(arr[0] != 0);
    assert(arr[i] >= 0);
    return arr[i];
}

int member_index_local(struct S *p)
{
    int local_idx = 1;
    assert(p->arr[local_idx] != 0);
    return p->arr[0];
}

int shadowed_formal(int w)
{
    {
        int w = 0;
        assert(w >= 0);
        return w;
    }
}
