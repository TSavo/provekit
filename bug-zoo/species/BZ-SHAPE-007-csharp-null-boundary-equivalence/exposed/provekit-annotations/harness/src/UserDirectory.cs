namespace BugZoo.CSharpNullBoundary.ProvekitAnnotations;

public static class UserDirectory
{
    //provekit:contract
    public static string Lookup(string name) => "user:" + name.ToUpperInvariant();
}
