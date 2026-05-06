// Forward-propagation floor fixture for Java
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

public class FloorFixture {
    public static boolean checkPositive(int x) {
        if (x <= 0) { return false; }  // pre: x > 0
        return true;
    }

    public static boolean callerSatisfiesPre() {
        boolean result = checkPositive(5);  // satisfies pre (x=5 > 0)
        return result;
    }

    public static boolean callerViolatesPre() {
        boolean result = checkPositive(-1);  // violates pre (x=-1 <= 0)
        return result;
    }

    public static boolean callerWithLoop() {
        for (int i = 0; i < 10; i++) {
            boolean result = checkPositive(i);  // top fallback at loop entry
            if (!result) { return false; }
        }
        return true;
    }
}