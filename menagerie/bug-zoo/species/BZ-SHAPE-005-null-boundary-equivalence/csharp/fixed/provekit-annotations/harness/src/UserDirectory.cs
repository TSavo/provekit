using System;

namespace BugZoo.CSharpNullBoundary.ProvekitAnnotations;

public static class UserDirectory
{
    //provekit:contract
    public static string Lookup(string? name)
    {
        if (name is null)
        {
            throw new ArgumentNullException(nameof(name), "name must be non-null");
        }

        return "user:" + name.ToUpperInvariant();
    }
}
