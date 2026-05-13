using System.Collections.Generic;
using System.Linq;

public class Demo
{
    public static IEnumerable<string?> Filter(IEnumerable<string?> names)
    {
        var nonNull = names.Where(name => name != null);
        return nonNull;
    }
}
