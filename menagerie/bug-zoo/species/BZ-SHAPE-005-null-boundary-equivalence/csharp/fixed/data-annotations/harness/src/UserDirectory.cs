using System;
using System.ComponentModel.DataAnnotations;

namespace BugZoo.CSharpNullBoundary.DataAnnotations;

public sealed class LookupRequest
{
    [Required]
    public string Name { get; init; } = "";
}

public static class UserDirectory
{
    public static string Lookup(string? name)
    {
        if (name is null)
        {
            throw new ArgumentNullException(nameof(name), "name must be non-null");
        }

        return "user:" + name.ToUpperInvariant();
    }
}
