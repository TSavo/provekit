/*
 * Effects extraction fixture, one function per CCP section 3 effect kind.
 *
 * pure_function exercises the empty-effect-set case. Every other
 * function exercises exactly one effect kind primarily; conservative
 * overlap is acceptable.
 */

struct ops {
    int (*method)(int);
};

struct device_state {
    int counter;
    int flag;
};

static struct device_state g_state;

void copy_to_user(const void *dst, const void *src, unsigned long n);
void *kmalloc(unsigned long size, unsigned int flags);
void BUG_ON(int condition);

int pure_function(int x, int y) {
    return x + y;
}

void writes_function(int v) {
    g_state.counter = v;
}

int reads_function(void) {
    return g_state.counter;
}

void io_function(const void *buf) {
    (void)kmalloc(64, 0);
    (void)buf;
}

void unsafe_function(void *p) {
    int *ip = (int *)p;
    (void)ip;
}

int panics_function(int v) {
    BUG_ON(v < 0);
    return v;
}

int unresolved_call_function(struct ops *o, int x) {
    return o->method(x);
}
