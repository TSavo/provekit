// Fixture vendor: commons-codec STRUCTURE (two static-final 64-entry tables, a
// ternary selector, a 3-byte accumulation, a 4-extraction full block) BUT the
// index expression of the first extraction contains a METHOD CALL:
// scramble(ibitWorkArea) >> 18 & MASK_6BITS. The strong-tier symbolic store
// cannot interpret a method call inside the index -> it must REFUSE BY NAME.
// The weak charset row is unaffected (the table walk does not read the index).
public class BadShapeCodec {

    private static final byte[] A_TABLE = {
        'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P',
        'Q','R','S','T','U','V','W','X','Y','Z','a','b','c','d','e','f',
        'g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v',
        'w','x','y','z','0','1','2','3','4','5','6','7','8','9','+','/'
    };
    private static final byte[] B_TABLE = {
        'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P',
        'Q','R','S','T','U','V','W','X','Y','Z','a','b','c','d','e','f',
        'g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v',
        'w','x','y','z','0','1','2','3','4','5','6','7','8','9','-','_'
    };
    private static final int MASK_6BITS = 0x3f;

    private final byte[] encodeTable;
    private int ibitWorkArea;
    private int pos;
    private final byte[] buffer = new byte[16];

    public BadShapeCodec(final boolean urlsafe) {
        this.encodeTable = urlsafe ? B_TABLE : A_TABLE;
    }

    private static int scramble(int x) { return x; }  // a method call the store can't interpret

    // The full-block shape lives in a method named `encode` so the strong walker
    // selects it as the candidate -- and then the interpret step hits the method
    // call inside the index and refuses by name.
    void encode(final byte[] in) {
        for (int i = 0; i < in.length; i++) {
            int b = in[i];
            if (b < 0) b += 256;
            ibitWorkArea = (ibitWorkArea << 8) + b;
            if ((i + 1) % 3 == 0) {
                // First extraction wraps the work area in a METHOD CALL:
                buffer[pos++] = encodeTable[scramble(ibitWorkArea) >> 18 & MASK_6BITS];
                buffer[pos++] = encodeTable[ibitWorkArea >> 12 & MASK_6BITS];
                buffer[pos++] = encodeTable[ibitWorkArea >> 6 & MASK_6BITS];
                buffer[pos++] = encodeTable[ibitWorkArea & MASK_6BITS];
            }
        }
    }

    String render() {
        return new String(buffer, 0, pos);
    }

    public static String encodeString(final byte[] data) {
        BadShapeCodec c = new BadShapeCodec(false);
        c.encode(data);
        return c.render();
    }
}
