// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance tests for the CICP golden vectors under
// protocol/conformance/cicp/. The bodies are parsed into the native
// canonicalizer Value tree, JCS-encoded, and hashed through the C# BLAKE3-512
// path so this pins the language kit behavior rather than raw fixture bytes.

using System.Text.Json;
using Provekit.Canonicalizer;
using Xunit;

namespace Provekit.Tests;

public class CicpConformanceTests
{
    private static readonly string CicpConformanceDir = FindCicpConformanceDir();

    [Fact]
    public void PassingVectors_DeriveExpectedCids()
    {
        foreach (var vector in ReadVectors().Where(v => v.ShouldPass))
        {
            var body = ReadBody(vector.Body);
            var actual = Hash.Blake3_512(Jcs.EncodeUtf8(JsonToValue(body.RootElement)));

            Assert.Equal(vector.ExpectedCid, actual);
            Assert.Empty(FindMissingInputCids(body.RootElement));
        }
    }

    [Fact]
    public void InvalidVector_FailsClosedWhenInputCidsOmitRequiredDependency()
    {
        var vector = Assert.Single(ReadVectors(), v => !v.ShouldPass);
        var body = ReadBody(vector.Body);

        var missing = FindMissingInputCids(body.RootElement).ToArray();

        Assert.Contains(missing, message => vector.ErrorContains is not null && message.Contains(vector.ErrorContains, StringComparison.Ordinal));
        Assert.NotEmpty(missing);
    }

    private static IReadOnlyList<CicpVector> ReadVectors()
    {
        using var doc = JsonDocument.Parse(File.ReadAllText(Path.Join(CicpConformanceDir, "vectors.json")));
        return doc.RootElement.GetProperty("vectors")
            .EnumerateArray()
            .Select(v => new CicpVector(
                v.GetProperty("name").GetString() ?? throw new InvalidOperationException("vector missing name"),
                v.GetProperty("body").GetString() ?? throw new InvalidOperationException("vector missing body"),
                v.GetProperty("shouldPass").GetBoolean(),
                v.TryGetProperty("expectedCid", out var expectedCid) ? expectedCid.GetString() : null,
                v.TryGetProperty("errorContains", out var errorContains) ? errorContains.GetString() : null))
            .ToArray();
    }

    private static JsonDocument ReadBody(string fileName) =>
        JsonDocument.Parse(File.ReadAllText(Path.Join(CicpConformanceDir, fileName)));

    private static Value JsonToValue(JsonElement element)
    {
        return element.ValueKind switch
        {
            JsonValueKind.Object => Value.Object(
                element.EnumerateObject().Select(p => new KeyValuePair<string, Value>(p.Name, JsonToValue(p.Value)))),
            JsonValueKind.Array => Value.Array(element.EnumerateArray().Select(JsonToValue)),
            JsonValueKind.String => Value.String(element.GetString() ?? string.Empty),
            JsonValueKind.Number => Value.Integer(element.GetInt64()),
            JsonValueKind.True => Value.True,
            JsonValueKind.False => Value.False,
            JsonValueKind.Null => Value.Null,
            _ => throw new InvalidOperationException($"Unsupported JSON value kind {element.ValueKind}"),
        };
    }

    private static IEnumerable<string> FindMissingInputCids(JsonElement body)
    {
        var inputCids = body.TryGetProperty("inputCids", out var inputCidsElement)
            ? inputCidsElement.EnumerateArray().Select(e => e.GetString()).OfType<string>().ToHashSet(StringComparer.Ordinal)
            : new HashSet<string>(StringComparer.Ordinal);

        return FindRequiredInputCids(body)
            .Where(cid => !inputCids.Contains(cid))
            .Distinct(StringComparer.Ordinal)
            .Select(cid => $"inputCids missing required CID {cid}");
    }

    private static IEnumerable<string> FindRequiredInputCids(JsonElement element, string? propertyName = null)
    {
        switch (element.ValueKind)
        {
            case JsonValueKind.Object:
                foreach (var property in element.EnumerateObject())
                {
                    if (property.NameEquals("inputCids"))
                    {
                        continue;
                    }

                    foreach (var cid in FindRequiredInputCids(property.Value, property.Name))
                    {
                        yield return cid;
                    }
                }
                break;

            case JsonValueKind.Array:
                foreach (var item in element.EnumerateArray())
                {
                    foreach (var cid in FindRequiredInputCids(item, propertyName))
                    {
                        yield return cid;
                    }
                }
                break;

            case JsonValueKind.String:
                var value = element.GetString();
                if (value is not null && IsCidDependencyField(propertyName) && IsBlake3Cid(value))
                {
                    yield return value;
                }
                break;
        }
    }

    private static bool IsCidDependencyField(string? propertyName) =>
        propertyName is not null && (propertyName.EndsWith("Cid", StringComparison.Ordinal) ||
                                     propertyName.EndsWith("Cids", StringComparison.Ordinal));

    private static bool IsBlake3Cid(string value) =>
        value.StartsWith(Hash.Blake3_512_Prefix, StringComparison.Ordinal) &&
        value.Length == Hash.Blake3_512_Prefix.Length + 128;

    private static string FindCicpConformanceDir()
    {
        for (var dir = new DirectoryInfo(Directory.GetCurrentDirectory()); dir is not null; dir = dir.Parent)
        {
            var candidate = Path.Join(dir.FullName, "protocol", "conformance", "cicp");
            if (File.Exists(Path.Join(candidate, "vectors.json")))
            {
                return candidate;
            }
        }

        throw new DirectoryNotFoundException("Could not locate protocol/conformance/cicp/vectors.json");
    }

    private sealed record CicpVector(
        string Name,
        string Body,
        bool ShouldPass,
        string? ExpectedCid,
        string? ErrorContains);
}
