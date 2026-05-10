/**
 * provekit_blake3: Ruby C extension wrapping vendored BLAKE3.
 *
 * Exposes three methods on Provekit::Blake3 (class-level):
 *   Blake3.hasher_init                            → Ruby String (sizeof(blake3_hasher) bytes,
 *                                                                typically 1912; carries
 *                                                                hasher state in-place)
 *   Blake3.hasher_update(hasher_str, data_str)    → nil  (mutates hasher_str's buffer)
 *   Blake3.hasher_finalize(hasher_str, out_len)   → raw bytes string of length out_len
 *
 * Statically links blake3.c + blake3_portable.c + blake3_dispatch.c
 * from tools/blake3-vendored/. Zero system deps.
 *
 * Memory model: the Ruby String returned by hasher_init owns the hasher
 * state in its character buffer. Ruby's GC frees the string when it
 * goes out of scope; we never call ruby_xmalloc / ruby_xfree on the
 * hasher pointer ourselves. update/finalize call rb_str_modify before
 * mutating to handle copy-on-write / frozen-string sharing.
 *
 * Wrapper functions are named provekit_rb_blake3_hasher_* to avoid
 * symbol collision with the BLAKE3 C library's own
 * blake3_hasher_init/update/finalize functions exported by blake3.h.
 */

#include <ruby.h>
#include "blake3.h"

/* ── Module declarations ──────────────────────────── */

static VALUE rb_cBlake3;

/* ── hasher_init(self) → Ruby String ─────────────────── */

static VALUE
provekit_rb_blake3_hasher_init(VALUE self)
{
    (void)self;
    VALUE str = rb_str_new(NULL, sizeof(blake3_hasher));
    blake3_hasher_init((blake3_hasher *)RSTRING_PTR(str));
    return str;
}

/* ── hasher_update(self, hasher_str, data_str) → nil ── */

static VALUE
provekit_rb_blake3_hasher_update(VALUE self, VALUE hasher_str, VALUE data_str)
{
    (void)self;
    Check_Type(hasher_str, T_STRING);
    Check_Type(data_str, T_STRING);

    if ((size_t)RSTRING_LEN(hasher_str) != sizeof(blake3_hasher)) {
        rb_raise(rb_eArgError,
                 "hasher_str length %ld does not match sizeof(blake3_hasher)=%zu",
                 (long)RSTRING_LEN(hasher_str), sizeof(blake3_hasher));
    }

    /* Make hasher_str's buffer independent (handles copy-on-write /
     * shared / frozen string buffers) before mutating. */
    rb_str_modify(hasher_str);

    blake3_hasher *h = (blake3_hasher *)RSTRING_PTR(hasher_str);
    blake3_hasher_update(h, RSTRING_PTR(data_str), RSTRING_LEN(data_str));

    return Qnil;
}

/* ── hasher_finalize(self, hasher_str, out_len_fixnum) → String ─── */

static VALUE
provekit_rb_blake3_hasher_finalize(VALUE self, VALUE hasher_str, VALUE out_len_val)
{
    (void)self;
    Check_Type(hasher_str, T_STRING);

    if ((size_t)RSTRING_LEN(hasher_str) != sizeof(blake3_hasher)) {
        rb_raise(rb_eArgError,
                 "hasher_str length %ld does not match sizeof(blake3_hasher)=%zu",
                 (long)RSTRING_LEN(hasher_str), sizeof(blake3_hasher));
    }

    blake3_hasher *h = (blake3_hasher *)RSTRING_PTR(hasher_str);
    size_t out_len = (size_t)NUM2ULONG(out_len_val);

    VALUE out_str = rb_str_new(NULL, out_len);
    blake3_hasher_finalize(h, (uint8_t *)RSTRING_PTR(out_str), out_len);

    /* No ruby_xfree here: hasher_str's buffer is owned by the Ruby
     * String, freed by GC when the string goes out of scope. The
     * previous code ruby_xfree'd RSTRING_PTR(hasher_str), which
     * freed memory the GC also tries to free: invalid free. */

    return out_str;
}

/* ── Init ──────────────────────────────────────────── */

/* Init function name MUST match the create_makefile basename in
 * extconf.rb (`provekit_blake3`). Ruby's extension loader looks for
 * Init_<basename> when loading the .so; mismatched names mean the
 * methods never register and `Provekit::Blake3.hasher_init` is undef. */
void
Init_provekit_blake3(void)
{
    rb_cBlake3 = rb_define_module("Provekit");
    VALUE rb_cBlake3Inner = rb_define_class_under(rb_cBlake3, "Blake3", rb_cObject);

    rb_define_singleton_method(rb_cBlake3Inner, "hasher_init",    RUBY_METHOD_FUNC(provekit_rb_blake3_hasher_init),    0);
    rb_define_singleton_method(rb_cBlake3Inner, "hasher_update",  RUBY_METHOD_FUNC(provekit_rb_blake3_hasher_update),  2);
    rb_define_singleton_method(rb_cBlake3Inner, "hasher_finalize",RUBY_METHOD_FUNC(provekit_rb_blake3_hasher_finalize),2);
}
