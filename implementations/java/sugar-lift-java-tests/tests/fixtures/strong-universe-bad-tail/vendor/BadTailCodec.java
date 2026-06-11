// Fixture vendor: a base64-shaped codec whose FULL block is walkable (so the
// strong tier emits for multiple-of-3 inputs) but whose mod-3 TAIL extraction
// wraps the work area in a METHOD CALL: scramble(ibitWorkArea) >> 10 & MASK.
// The tail index is uninterpretable -> a non-multiple-of-3 callsite must REFUSE
// the strong row BY NAME (modulus-N tail), while the weak charset row stands
// and multiple-of-3 callsites still get the full-block strong row.
public class BadTailCodec {

    private static final byte[] STANDARD_ENCODE_TABLE = {
        'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P',
        'Q','R','S','T','U','V','W','X','Y','Z','a','b','c','d','e','f',
        'g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v',
        'w','x','y','z','0','1','2','3','4','5','6','7','8','9','+','/'
    };
    private static final byte[] URL_SAFE_ENCODE_TABLE = {
        'A','B','C','D','E','F','G','H','I','J','K','L','M','N','O','P',
        'Q','R','S','T','U','V','W','X','Y','Z','a','b','c','d','e','f',
        'g','h','i','j','k','l','m','n','o','p','q','r','s','t','u','v',
        'w','x','y','z','0','1','2','3','4','5','6','7','8','9','-','_'
    };
    private static final int MASK_6BITS = 0x3f;
    private static final byte PAD_DEFAULT = '=';

    private final byte[] encodeTable;
    private final byte pad = PAD_DEFAULT;
    private int ibitWorkArea;
    private int modulus;
    private int pos;
    private final byte[] buffer = new byte[64];

    public BadTailCodec(final boolean urlsafe) {
        this.encodeTable = urlsafe ? URL_SAFE_ENCODE_TABLE : STANDARD_ENCODE_TABLE;
    }

    private static int scramble(int x) { return x; }  // uninterpretable in an index

    void encode(final byte[] in, final int avail) {
        if (avail < 0) {
            switch (modulus) {
                case 1:
                    // TAIL index wraps the work area in a METHOD CALL -> refuse.
                    buffer[pos++] = encodeTable[scramble(ibitWorkArea) >> 2 & MASK_6BITS];
                    buffer[pos++] = encodeTable[ibitWorkArea << 4 & MASK_6BITS];
                    if (encodeTable == STANDARD_ENCODE_TABLE) {
                        buffer[pos++] = pad;
                        buffer[pos++] = pad;
                    }
                    break;
                case 2:
                    buffer[pos++] = encodeTable[scramble(ibitWorkArea) >> 10 & MASK_6BITS];
                    buffer[pos++] = encodeTable[ibitWorkArea >> 4 & MASK_6BITS];
                    buffer[pos++] = encodeTable[ibitWorkArea << 2 & MASK_6BITS];
                    if (encodeTable == STANDARD_ENCODE_TABLE) {
                        buffer[pos++] = pad;
                    }
                    break;
            }
            return;
        }
        for (int i = 0; i < avail; i++) {
            modulus = (modulus + 1) % 3;
            int b = in[i];
            if (b < 0) b += 256;
            ibitWorkArea = (ibitWorkArea << 8) + b;
            if (0 == modulus) {
                // FULL block: walkable (no method call). The strong tier emits here.
                buffer[pos++] = encodeTable[ibitWorkArea >> 18 & MASK_6BITS];
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
        BadTailCodec c = new BadTailCodec(false);
        c.encode(data, data.length);
        c.encode(data, -1);
        return c.render();
    }
}
