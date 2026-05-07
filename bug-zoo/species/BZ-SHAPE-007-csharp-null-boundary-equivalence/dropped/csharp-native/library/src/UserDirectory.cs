namespace BugZoo.CSharpNullBoundary;

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
