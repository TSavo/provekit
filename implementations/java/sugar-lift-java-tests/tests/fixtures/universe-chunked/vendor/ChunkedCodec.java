// H1 [B6] fixture: codec that combines a table selector with a lineLength parameter.
// The walk traces the table selector (the 'lower' boolean ternary) but must also
// detect that a non-zero lineLength injects CHUNK_SEPARATOR chars (not in any table)
// into the output — making the str.chars-in-set universe contract unsound.
//
// encodeNoChunk: passes lower=false, lineLength=0 → table=UPPER_TABLE, no separator.
//   Walk: selector resolves to UPPER_TABLE, lineLength=0 binds → separator guard false.
//   MUST register with UPPER_TABLE chars.
//
// encodeChunked: passes lower=false, lineLength=76 → table=UPPER_TABLE, separator injected.
//   Walk: selector resolves to UPPER_TABLE, lineLength=76 binds → separator guard true.
//   The output contains '\r' and '\n' (from CHUNK_SEPARATOR) — NOT in UPPER_TABLE.
//   MUST REFUSE: str.chars-in-set(UPPER_TABLE) would be a false axiom.
public class ChunkedCodec {

    private static final byte[] UPPER_TABLE = { 'A', 'B', 'C', 'D' };
    private static final byte[] LOWER_TABLE = { 'a', 'b', 'c', 'd' };
    private static final byte[] CHUNK_SEPARATOR = { '\r', '\n' };

    private final byte[] encodeTable;
    private final int lineLength;

    public ChunkedCodec(final boolean lower, final int lineLength) {
        this.encodeTable = lower ? LOWER_TABLE : UPPER_TABLE;
        this.lineLength = lineLength;
    }

    String encode(final byte[] data) {
        final byte[] buffer = new byte[16];
        buffer[0] = encodeTable[0];
        if (lineLength > 0) {
            // Chunked: line separator appended — NOT in encodeTable.
            // str.chars-in-set(encodeTable) would be wrong for this path.
            buffer[1] = CHUNK_SEPARATOR[0]; // '\r'
            buffer[2] = CHUNK_SEPARATOR[1]; // '\n'
        }
        return new String(buffer);
    }

    public static String encodeNoChunk(final byte[] data) {
        return encodeWithParams(data, false, 0);
    }

    public static String encodeChunked(final byte[] data) {
        return encodeWithParams(data, false, 76);
    }

    static String encodeWithParams(final byte[] data, final boolean lower, final int lineLength) {
        final ChunkedCodec codec = new ChunkedCodec(lower, lineLength);
        return codec.encode(data);
    }
}
