using System.ComponentModel.DataAnnotations;

namespace BugZoo.CSharpNullBoundary.DataAnnotations;

public sealed class LookupRequest
{
    [Required]
    public string Name { get; init; } = "";
}

public static class UserDirectory
{
    public static string Lookup(string name) => "user:" + name.ToUpperInvariant();
}
