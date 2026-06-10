// Fixture vendor file: public static int method with a single-statement
// non-ternary body.  The numeric-universe walker must refuse by name.
public class MyMath {
    /** NOT the supported ternary shape: multiply is not ternary-with-comparison. */
    public static int square(int a) {
        return a * a;
    }
}
