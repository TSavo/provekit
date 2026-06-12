import org.junit.Test;
import static org.junit.Assert.assertEquals;
import static account.Handles.accept;
import static account.Handles.risky;
import static account.Handles.dynamic;

/**
 * Fixture test for Door 3 @Pattern regex-universe lifting.
 *
 *  - accept("alice_01"): the @Pattern is REGULAR → an equality contract AND a
 *    str.in-regex universe row under the SAME #euf# name.
 *  - risky("aa"): the @Pattern uses a backreference → REFUSED BY NAME; only the
 *    weak equality, NO str.in-regex row.
 *  - dynamic("abc"): the @Pattern regexp is a non-literal (constant ref) → not
 *    walked; only the weak equality, NO str.in-regex row.
 */
public class RegexUniverseLift {

    @Test
    public void acceptedHandle() {
        assertEquals("alice_01", accept("alice_01"));
    }

    @Test
    public void backreferencePattern() {
        assertEquals("aa", risky("aa"));
    }

    @Test
    public void nonLiteralPattern() {
        assertEquals("abc", dynamic("abc"));
    }
}
