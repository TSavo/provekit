// Fixture: MUTABLE-TABLE discrimination.
// Identical structure to MiniCodec EXCEPT the upper table is missing `final`.
// A mutable table is no axiom: the walker must refuse the selector by name
// and emit NO universe row. The equality contract still lifts.
public class MutableCodec {

    private static byte[] UPPER_TABLE = { 'A', 'B', 'C' }; // NOT final — no axiom
    private static final byte[] LOWER_TABLE = { 'a', 'b', 'c' };
    private static final byte pad = '=';

    private final byte[] outTable;

    public MutableCodec(final boolean lower) {
        this.outTable = lower ? LOWER_TABLE : UPPER_TABLE;
    }

    String render(final byte[] data) {
        final byte[] buffer = new byte[8];
        buffer[0] = outTable[0];
        if (outTable == UPPER_TABLE) {
            buffer[1] = pad;
        }
        return new String(buffer);
    }

    public static String encodeUpper(final byte[] data) {
        return encode(data, false);
    }

    static String encode(final byte[] data, final boolean lower) {
        final MutableCodec codec = new MutableCodec(lower);
        return codec.render(data);
    }
}
