/**
 * provekit_blake3 — Ruby C extension wrapping vendored BLAKE3.
 *
 * Exposes three methods on Provekit::Blake3 (class-level):
 *   Blake3.hasher_init        → FFI::MemoryPointer (1 blake3_hasher, 4096 bytes)
 *   Blake3.hasher_update(ptr, data_string) → nil
 *   Blake3.hasher_finalize(ptr, out_len)   → raw bytes string
 *
 * Statically links blake3.c + blake3_portable.c + blake3_dispatch.c
 * from tools/blake3-vendored/. Zero system deps.
 */

#include <ruby.h>
#include "blake3.h"

#define HASHER_SIZE 4096

/* ── Module declarations ──────────────────────────── */

static VALUE rb_cBlake3;

/* ── hasher_init(self) → FFI::MemoryPointer ──────────── */

static VALUE
blake3_hasher_init(VALUE self)
{
    blake3_hasher *h = (blake3_hasher *)ruby_xmalloc(sizeof(blake3_hasher));
    blake3_hasher_init(h);
    return rb_str_new((const char *)h, sizeof(blake3_hasher));
}

/* ── hasher_update(self, hasher_str, data_str) → nil ── */

static VALUE
blake3_hasher_update(VALUE self, VALUE hasher_str, VALUE data_str)
{
    Check_Type(hasher_str, T_STRING);
    Check_Type(data_str, T_STRING);

    blake3_hasher *h = (blake3_hasher *)RSTRING_PTR(hasher_str);
    blake3_hasher_update(h, RSTRING_PTR(data_str), RSTRING_LEN(data_str));

    return Qnil;
}

/* ── hasher_finalize(self, hasher_str, out_len_fixnum) → String ─── */

static VALUE
blake3_hasher_finalize(VALUE self, VALUE hasher_str, VALUE out_len_val)
{
    Check_Type(hasher_str, T_STRING);

    blake3_hasher *h = (blake3_hasher *)RSTRING_PTR(hasher_str);
    size_t out_len = (size_t)NUM2ULONG(out_len_val);

    VALUE out_str = rb_str_new(NULL, out_len);
    blake3_hasher_finalize(h, (uint8_t *)RSTRING_PTR(out_str), out_len);

    /* Free the hasher — finalize consumes it */
    ruby_xfree(h);

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

    rb_define_singleton_method(rb_cBlake3Inner, "hasher_init",    RUBY_METHOD_FUNC(blake3_hasher_init),    0);
    rb_define_singleton_method(rb_cBlake3Inner, "hasher_update",  RUBY_METHOD_FUNC(blake3_hasher_update),  2);
    rb_define_singleton_method(rb_cBlake3Inner, "hasher_finalize",RUBY_METHOD_FUNC(blake3_hasher_finalize),2);
}
