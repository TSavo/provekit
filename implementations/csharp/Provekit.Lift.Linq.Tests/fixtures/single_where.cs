using System.Collections.Generic;
using System.Linq;

public class Demo
{
    public static IEnumerable<int> Filter(IEnumerable<int> xs)
    {
        var positives = xs.Where(x => x > 0);
        return positives;
    }
}
