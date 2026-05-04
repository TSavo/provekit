// SPDX-License-Identifier: Apache-2.0
//
// Tiny JSON value tree used by the JCS encoder. Java peer of
// implementations/csharp/Provekit.Canonicalizer/Value.cs and
// implementations/rust/provekit-canonicalizer/src/value.rs.
//
// Insertion-order is preserved for objects; the JCS encoder re-sorts
// keys at emit time per RFC 8785 §3.2.3 (Unicode code-point order; for
// ASCII keys this collapses to byte-order).

package com.provekit.canonicalizer;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;

/**
 * Immutable JSON value used by the canonicalizer. Construct via the
 * static factory methods {@link #ofNull()}, {@link #ofBool(boolean)},
 * {@link #ofInt(long)}, {@link #ofString(String)}, {@link #ofArray(List)},
 * and {@link #ofObject(List)}.
 */
public final class Value {

    public enum Kind { NULL, BOOL, INTEGER, STRING, ARRAY, OBJECT }

    private final Kind kind;
    private final boolean boolValue;
    private final long intValue;
    private final String strValue;
    private final List<Value> arrValue;
    private final List<Map.Entry<String, Value>> objValue;

    private Value(Kind kind, boolean b, long i, String s,
                  List<Value> arr, List<Map.Entry<String, Value>> obj) {
        this.kind = kind;
        this.boolValue = b;
        this.intValue = i;
        this.strValue = s;
        this.arrValue = arr;
        this.objValue = obj;
    }

    private static final Value NULL = new Value(Kind.NULL, false, 0L, null, null, null);
    private static final Value TRUE = new Value(Kind.BOOL, true, 0L, null, null, null);
    private static final Value FALSE = new Value(Kind.BOOL, false, 0L, null, null, null);

    public static Value ofNull() { return NULL; }

    public static Value ofBool(boolean b) { return b ? TRUE : FALSE; }

    public static Value ofInt(long n) {
        return new Value(Kind.INTEGER, false, n, null, null, null);
    }

    public static Value ofString(String s) {
        if (s == null) {
            throw new NullPointerException("string value must not be null");
        }
        return new Value(Kind.STRING, false, 0L, s, null, null);
    }

    public static Value ofArray(List<Value> items) {
        if (items == null) {
            throw new NullPointerException("array items must not be null");
        }
        return new Value(Kind.ARRAY, false, 0L, null,
            Collections.unmodifiableList(new ArrayList<>(items)), null);
    }

    public static Value ofArray(Value... items) {
        return ofArray(Arrays.asList(items));
    }

    public static Value ofObject(List<Map.Entry<String, Value>> entries) {
        if (entries == null) {
            throw new NullPointerException("object entries must not be null");
        }
        ArrayList<Map.Entry<String, Value>> copy = new ArrayList<>(entries.size());
        for (Map.Entry<String, Value> e : entries) {
            if (e.getKey() == null) {
                throw new NullPointerException("object key must not be null");
            }
            if (e.getValue() == null) {
                throw new NullPointerException("object value must not be null");
            }
            copy.add(Map.entry(e.getKey(), e.getValue()));
        }
        return new Value(Kind.OBJECT, false, 0L, null, null, Collections.unmodifiableList(copy));
    }

    public Kind kind() { return kind; }

    public boolean asBool() {
        if (kind != Kind.BOOL) throw new IllegalStateException("not bool");
        return boolValue;
    }

    public long asInt() {
        if (kind != Kind.INTEGER) throw new IllegalStateException("not integer");
        return intValue;
    }

    public String asString() {
        if (kind != Kind.STRING) throw new IllegalStateException("not string");
        return strValue;
    }

    public List<Value> asArray() {
        if (kind != Kind.ARRAY) throw new IllegalStateException("not array");
        return arrValue;
    }

    public List<Map.Entry<String, Value>> asObject() {
        if (kind != Kind.OBJECT) throw new IllegalStateException("not object");
        return objValue;
    }
}
