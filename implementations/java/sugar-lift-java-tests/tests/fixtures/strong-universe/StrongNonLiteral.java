// Fixture: non-literal input -> no known byte length -> NO strong row.
// (The weak walker names its own refusal for the non-literal call arg.)
import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static org.apache.commons.codec.binary.Base64.encodeBase64String;

public class StrongNonLiteral {

    @Test
    public void testNonLiteralInputNoStrong() {
        byte[] data = "bar".getBytes();
        // 'data' is a non-literal local: the strong gate never fires.
        assertEquals("YmFy", encodeBase64String(data));
    }
}
