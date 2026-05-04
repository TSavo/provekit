// SPDX-License-Identifier: Apache-2.0

package com.provekit.claimenvelope;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;

import java.io.ByteArrayOutputStream;

import org.junit.jupiter.api.Test;

/**
 * Mirrors implementations/rust/provekit-proof-envelope/src/cbor.rs unit tests.
 */
class CborTest {

    @Test
    void shortest_form_uint_zero() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeUint(o, 0);
        assertArrayEquals(new byte[]{0x00}, o.toByteArray());
    }

    @Test
    void shortest_form_uint_23() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeUint(o, 23);
        assertArrayEquals(new byte[]{0x17}, o.toByteArray());
    }

    @Test
    void shortest_form_uint_24() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeUint(o, 24);
        assertArrayEquals(new byte[]{0x18, 24}, o.toByteArray());
    }

    @Test
    void shortest_form_uint_256() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeUint(o, 256);
        assertArrayEquals(new byte[]{0x19, 0x01, 0x00}, o.toByteArray());
    }

    @Test
    void shortest_form_uint_65536() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeUint(o, 65536);
        assertArrayEquals(new byte[]{0x1a, 0x00, 0x01, 0x00, 0x00}, o.toByteArray());
    }

    @Test
    void tstr_round_trip_head() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeTstr(o, "hello");
        // major 3 (text string), len 5 short form: 0x65, then "hello"
        assertArrayEquals(new byte[]{0x65, 'h', 'e', 'l', 'l', 'o'}, o.toByteArray());
    }

    @Test
    void map_head_seven() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeMapHead(o, 7);
        assertArrayEquals(new byte[]{(byte) 0xa7}, o.toByteArray());
    }

    @Test
    void bstr_short() {
        ByteArrayOutputStream o = new ByteArrayOutputStream();
        Cbor.encodeBstr(o, new byte[]{1, 2, 3});
        // major 2 (byte string), len 3 short form: 0x43, then 1 2 3
        assertArrayEquals(new byte[]{0x43, 1, 2, 3}, o.toByteArray());
    }
}
