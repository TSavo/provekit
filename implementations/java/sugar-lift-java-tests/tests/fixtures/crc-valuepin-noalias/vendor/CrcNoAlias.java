package vendor;

/**
 * DISCRIMINATION fixture for the CRC value-pin rung. The static-init table FOLDS
 * cleanly (the construction-site walk succeeds, like CRC32C), but update() reads
 * an alias field `byteTable` that is assigned ONLY a freshly-allocated
 * `new int[256]` — it is NEVER aliased to the folded sub-array byteTables[0].
 *
 * The value-pin walk MUST refuse by name: the `byteTable` alias is not statically
 * resolvable to a folded table sub-array. It must NOT guess a branch or fabricate
 * the table read. The construction-site table-fold is unaffected (additive).
 */
public final class CrcNoAlias {

    private static final int POLY = 0x82F63B78;
    private static final int[][] byteTables = new int[8][256];
    private static final int[] byteTable;

    static {
        for (int index = 0; index < byteTables[0].length; index++) {
            int r = index;
            for (int i = 0; i < 8; i++) {
                if ((r & 1) != 0) {
                    r = (r >>> 1) ^ POLY;
                } else {
                    r >>>= 1;
                }
            }
            byteTables[0][index] = r;
        }
        // The alias is assigned a FRESH array, never the folded sub-array. It is
        // not statically resolvable to a folded table — the value-pin must refuse.
        byteTable = new int[byteTables[0].length];
    }

    private int crc = 0xFFFFFFFF;

    public void update(int b) {
        crc = (crc >>> 8) ^ byteTable[(crc ^ (b & 0xFF)) & 0xFF];
    }

    public long getValue() {
        return (~crc) & 0xFFFFFFFFL;
    }
}
