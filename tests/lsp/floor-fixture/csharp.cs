// Forward-propagation floor fixture for C#
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

public class FloorFixture
{
    public static bool CheckPositive(int x)
    {
        if (x <= 0) return false;  // pre: x > 0
        return true;
    }

    public static bool CallerSatisfiesPre()
    {
        bool result = CheckPositive(5);  // satisfies pre (x=5 > 0)
        return result;
    }

    public static bool CallerViolatesPre()
    {
        bool result = CheckPositive(-1);  // violates pre (x=-1 <= 0)
        return result;
    }

    public static bool CallerWithLoop()
    {
        for (int i = 0; i < 10; i++)
        {
            bool result = CheckPositive(i);  // top fallback at loop entry
            if (!result) return false;
        }
        return true;
    }
}