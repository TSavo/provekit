using System.Collections.Generic;
using System.Linq;

public class Demo
{
    public static IEnumerable<int> QueryForm(IEnumerable<int> xs)
    {
        var positives = from x in xs where x > 0 select x;
        return positives;
    }

    public static IEnumerable<int> MethodForm(IEnumerable<int> xs)
    {
        var positives = xs.Where(x => x > 0).Select(x => x);
        return positives;
    }
}
