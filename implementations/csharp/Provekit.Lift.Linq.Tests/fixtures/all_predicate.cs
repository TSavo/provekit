using System.Collections.Generic;
using System.Linq;

public class Demo
{
    public static bool AllPositive(IEnumerable<int> xs)
    {
        var ok = xs.All(x => x > 0);
        return ok;
    }
}
