// Fixture: CHAIN-ESCAPE discrimination.
// The tables and selector are sound, but the public entry point delegates to
// a method that is NOT in the vendored corpus (Foreign.render). The walker
// must refuse the entry by name and emit NO universe row.
public class EscapeCodec {

    private static final byte[] UPPER_TABLE = { 'A', 'B', 'C' };
    private static final byte[] LOWER_TABLE = { 'a', 'b', 'c' };

    private final byte[] outTable;

    public EscapeCodec(final boolean lower) {
        this.outTable = lower ? LOWER_TABLE : UPPER_TABLE;
    }

    public static String encodeUpper(final byte[] data) {
        // Foreign is not vendored: the chain leaves walkable source here.
        return Foreign.render(data);
    }
}
