// P5c fixture: INSTANCE-METHOD call on a locally-constructed receiver → location-keyed.
// `Base64 codec = new Base64(); assertEquals(STRICT, codec.getCodecPolicy())`
// The receiver is a local variable → location-keyed ::assertion, NOT #euf#-federated.
// Two test methods with DIFFERENT constructions asserting different values about
// the same method name must NOT collide (each is location-keyed to its own scope).
import org.junit.Test;
import static org.junit.Assert.assertEquals;

public class P5cInstanceMethodLift {

    // codec is constructed locally → instance-method call → location-keyed.
    @Test
    public void testCodecPolicyDefault() {
        Codec codec = new Codec();
        assertEquals(0, codec.getPolicy());
    }

    // Different local construction, different expected value — must NOT collide with above.
    @Test
    public void testCodecPolicyStrict() {
        Codec codec = new Codec(1);
        assertEquals(1, codec.getPolicy());
    }

    static class Codec {
        private int policy;
        Codec() { this.policy = 0; }
        Codec(int p) { this.policy = p; }
        int getPolicy() { return policy; }
    }
}
