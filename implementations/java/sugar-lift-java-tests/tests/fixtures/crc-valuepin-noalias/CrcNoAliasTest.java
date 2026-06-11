package demo;

import vendor.CrcNoAlias;
import org.junit.Test;
import static org.junit.Assert.assertEquals;

/** Drives the no-alias CRC over a literal so the value-pin walk is attempted —
 *  and must REFUSE the alias by name (no value-pin contract emitted). */
public class CrcNoAliasTest {
    @Test
    public void t() {
        CrcNoAlias c = new CrcNoAlias();
        c.update('1');
        long v = c.getValue();
        assertEquals(0x00000000L, v);
    }
}
