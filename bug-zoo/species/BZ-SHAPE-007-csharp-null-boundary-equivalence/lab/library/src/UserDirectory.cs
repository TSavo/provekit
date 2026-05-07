namespace BugZoo.CSharpNullBoundary;

public static class UserDirectory
{
    public static string Lookup(string name) => "user:" + name.ToUpperInvariant();
}
