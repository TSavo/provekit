using System;
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

    public static string Lookup(string? name)
    {
        if (name is null)
        {
            throw new ArgumentNullException(nameof(name), "name must be non-null");
        }

        return "user:" + name.ToUpperInvariant();
    }
}
