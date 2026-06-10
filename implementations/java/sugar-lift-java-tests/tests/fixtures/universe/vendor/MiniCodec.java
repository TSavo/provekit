// Fixture: mini vendor implementation with the commons-codec STRUCTURE —
// two static final tables, a ternary selector keyed on a ctor boolean param,
// a pad write guarded by `==` against ONE table, and static entry points
// resolved by literal propagation. Nothing in the kit names these fields or
// methods; the walk discovers all of them.
public class MiniCodec {

    private static final byte[] UPPER_TABLE = { 'A', 'B', 'C' };
    private static final byte[] LOWER_TABLE = { 'a', 'b', 'c' };
    private static final byte pad = '=';

    private final byte[] outTable;

    public MiniCodec(final boolean lower) {
        this.outTable = lower ? LOWER_TABLE : UPPER_TABLE;
    }

    String render(final byte[] data) {
        final byte[] buffer = new byte[8];
        buffer[0] = outTable[0];
        // The vendor's own pad-attribution guard: only the upper table pads.
        if (outTable == UPPER_TABLE) {
            buffer[1] = pad;
        }
        return new String(buffer);
    }

    public static String encodeUpper(final byte[] data) {
        return encode(data, false);
    }

    public static String encodeLower(final byte[] data) {
        return encode(data, true);
    }

    static String encode(final byte[] data, final boolean lower) {
        final MiniCodec codec = new MiniCodec(lower);
        return codec.render(data);
    }
}
