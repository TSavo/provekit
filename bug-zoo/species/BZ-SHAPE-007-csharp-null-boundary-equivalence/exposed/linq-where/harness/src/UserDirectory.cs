using System.Collections.Generic;
using System.Linq;

namespace BugZoo.CSharpNullBoundary.LinqWhere;

public static class UserDirectory
{
    public static IEnumerable<string?> Filter(IEnumerable<string?> names)
    {
        var nonNull = names.Where(name => name != null);
        return nonNull;
    }

    public static string Lookup(string name) => "user:" + name.ToUpperInvariant();
}
