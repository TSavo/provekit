using BugZoo.CSharpNullBoundary;

var result = UserDirectory.Lookup("ada");
if (result != "user:ADA")
{
    throw new InvalidOperationException($"unexpected lookup result: {result}");
}
