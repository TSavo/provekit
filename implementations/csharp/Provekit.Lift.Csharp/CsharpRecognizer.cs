using System.Text.Json.Nodes;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

namespace Provekit.Lift.Csharp;

public static class CsharpRecognizer
{
    public static JsonObject RecognizeText(
        string source,
        string fileName,
        IReadOnlyList<JsonObject> bindingTemplates)
    {
        var tree = CSharpSyntaxTree.ParseText(source, path: fileName);
        var bindingsByCid = BindingTemplatesByCid(bindingTemplates);
        var tags = new JsonArray();

        foreach (var method in tree.GetRoot().DescendantNodes().OfType<MethodDeclarationSyntax>())
        {
            if (method.Body is null && method.ExpressionBody is null) continue;

            var astTemplate = CsharpAstTemplates.MethodBodyTemplate(method);
            var templateCid = CsharpAstTemplates.TemplateCid(astTemplate);
            if (!bindingsByCid.TryGetValue(templateCid, out var binding)) continue;

            tags.Add(RecognizeTag(fileName, method, templateCid, binding));
        }

        return new JsonObject { ["tags"] = tags };
    }

    public static JsonObject RecognizePaths(
        string projectRoot,
        IReadOnlyList<string> sourcePaths,
        IReadOnlyList<JsonObject> bindingTemplates)
    {
        if (string.IsNullOrWhiteSpace(projectRoot))
            throw new ArgumentException("missing `project_root`");

        var root = Path.GetFullPath(projectRoot);
        var tags = new JsonArray();

        foreach (var requestedPath in sourcePaths)
        {
            var fullPath = Path.GetFullPath(Path.IsPathRooted(requestedPath)
                ? requestedPath
                : Path.Combine(root, requestedPath));
            if (!IsInsideRoot(root, fullPath)) continue;

            IEnumerable<string> files;
            if (Directory.Exists(fullPath))
            {
                files = Directory.EnumerateFiles(fullPath, "*.cs", SearchOption.AllDirectories)
                    .OrderBy(path => path, StringComparer.Ordinal);
            }
            else if (File.Exists(fullPath) && string.Equals(Path.GetExtension(fullPath), ".cs", StringComparison.OrdinalIgnoreCase))
            {
                files = new[] { fullPath };
            }
            else
            {
                continue;
            }

            foreach (var file in files)
            {
                string source;
                try
                {
                    source = File.ReadAllText(file);
                }
                catch
                {
                    continue;
                }

                var relPath = NormalizePath(Path.GetRelativePath(root, file));
                var response = RecognizeText(source, relPath, bindingTemplates);
                foreach (var tag in response["tags"]!.AsArray())
                {
                    tags.Add(tag!.DeepClone());
                }
            }
        }

        return new JsonObject { ["tags"] = tags };
    }

    private static Dictionary<string, JsonObject> BindingTemplatesByCid(IReadOnlyList<JsonObject> bindingTemplates)
    {
        var bindingsByCid = new Dictionary<string, JsonObject>(StringComparer.Ordinal);
        foreach (var binding in bindingTemplates)
        {
            var cid = binding["template_cid"]?.GetValue<string>();
            if (string.IsNullOrWhiteSpace(cid) && binding["ast_template"] is { } astTemplate)
            {
                cid = CsharpAstTemplates.TemplateCid(astTemplate);
            }

            if (!string.IsNullOrWhiteSpace(cid))
            {
                bindingsByCid[cid] = binding;
            }
        }

        return bindingsByCid;
    }

    private static JsonObject RecognizeTag(
        string fileName,
        MethodDeclarationSyntax method,
        string templateCid,
        JsonObject binding)
    {
        return new JsonObject
        {
            ["file"] = fileName,
            ["span"] = SpanFor(method),
            ["function_name"] = method.Identifier.Text,
            ["concept_name"] = CloneOrNull(binding["concept_name"]),
            ["library_tag"] = CloneOrNull(binding["library_tag"]),
            ["family"] = CloneOrNull(binding["family"]),
            ["template_cid"] = templateCid,
            ["contract_cid"] = CloneOrNull(binding["contract_cid"]),
            ["match_tier"] = "exact",
            ["param_bindings"] = ParamBindings(method),
        };
    }

    private static JsonArray ParamBindings(MethodDeclarationSyntax method)
    {
        var bindings = new JsonArray();
        var parameters = method.ParameterList.Parameters;
        for (var i = 0; i < parameters.Count; i++)
        {
            bindings.Add(new JsonObject
            {
                ["index"] = i + 1,
                ["source_text"] = parameters[i].Identifier.Text,
            });
        }

        return bindings;
    }

    private static JsonObject SpanFor(SyntaxNode node)
    {
        var span = node.GetLocation().GetLineSpan();
        return new JsonObject
        {
            ["start_line"] = span.StartLinePosition.Line + 1,
            ["start_col"] = span.StartLinePosition.Character,
            ["end_line"] = span.EndLinePosition.Line + 1,
            ["end_col"] = span.EndLinePosition.Character,
        };
    }

    private static JsonNode? CloneOrNull(JsonNode? node) => node?.DeepClone();

    private static bool IsInsideRoot(string root, string path)
    {
        var normalizedRoot = root.EndsWith(Path.DirectorySeparatorChar)
            ? root
            : root + Path.DirectorySeparatorChar;
        var comparison = OperatingSystem.IsWindows()
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;
        return string.Equals(root, path, comparison)
            || path.StartsWith(normalizedRoot, comparison);
    }

    private static string NormalizePath(string path) => path.Replace(Path.DirectorySeparatorChar, '/');
}
