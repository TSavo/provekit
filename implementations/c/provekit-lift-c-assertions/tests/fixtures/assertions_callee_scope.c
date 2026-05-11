/* Regression fixture: callee-local and function-local state must not leak
 * into a caller's computed pre (over-extraction soundness fix).
 *
 * g has assert(y > 0) on its own formal and also computes local_of_g.
 * f calls g and has its own assert on its formal x.
 *
 * Expected lifter behaviour:
 *   - g emits a function-contract pre referencing g's formal "y".
 *   - f emits a function-contract pre referencing f's formal "x" only.
 *   - "local_of_g" must NOT appear in any function-contract pre.
 *   - h emits no hard contract (local-only assert); opacity emitted instead.
 */

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
