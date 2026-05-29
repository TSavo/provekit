using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Provekit.Lift.Csharp;

public static class CsharpBindingTemplateResolver
{
    public static IReadOnlyList<JsonObject> ResolveFromProject(string projectRoot)
    {
        var root = Path.GetFullPath(string.IsNullOrWhiteSpace(projectRoot) ? "." : projectRoot);
        if (!Directory.Exists(root)) return Array.Empty<JsonObject>();

        var templates = new List<JsonObject>();
        foreach (var proofPath in Directory.EnumerateFiles(root, "*.proof", SearchOption.AllDirectories)
                     .OrderBy(path => path, StringComparer.Ordinal))
        {
            try
            {
                templates.AddRange(LoadBindingTemplatesFromProof(proofPath));
            }
            catch
            {
                // A project can contain proofs for other kits or legacy shapes.
                // Recognition is best-effort over C# sugar entries only.
            }
        }

        return templates;
    }

    public static IReadOnlyList<JsonObject> LoadBindingTemplatesFromProof(string path)
    {
        var bytes = File.ReadAllBytes(path);
        var envelope = LooksLikeJson(bytes)
            ? JsonNode.Parse(bytes) ?? new JsonObject()
            : new CborJsonDecoder(bytes).Decode();

        var records = CollectMemberRecords(envelope).ToList();
        var contractByFunction = ContractCidsByFunction(records);
        var templates = new List<JsonObject>();

        foreach (var (_, raw) in records)
        {
            var record = UnwrapEnvelope(raw);
            if (record?["kind"]?.GetValue<string>() != "library-sugar-binding-entry")
                continue;
            if (record["target_language"]?.GetValue<string>() is { } targetLanguage
                && !string.Equals(targetLanguage, "csharp", StringComparison.OrdinalIgnoreCase))
                continue;

            var bodySource = record["body_source"] as JsonObject;
            if (bodySource?["ast_template"] is null) continue;

            var functionName = record["source_function_name"]?.GetValue<string>() ?? "";
            templates.Add(new JsonObject
            {
                ["concept_name"] = CloneOrNull(record["concept_name"]),
                ["library_tag"] = CloneOrNull(record["target_library_tag"]),
                ["family"] = CloneOrNull(record["family"]),
                ["ast_template"] = bodySource["ast_template"]!.DeepClone(),
                ["template_cid"] = CloneOrNull(bodySource["template_cid"]),
                ["param_names"] = CloneOrNull(bodySource["param_names"]),
                ["contract_cid"] = CloneOrNull(record["contract_cid"])
                    ?? (contractByFunction.TryGetValue(functionName, out var cid) ? JsonValue.Create(cid) : null),
                ["source_function_name"] = functionName,
            });
        }

        return templates;
    }

    private static Dictionary<string, string> ContractCidsByFunction(IEnumerable<(string Cid, JsonNode Record)> records)
    {
        var contractByFunction = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var (cid, raw) in records)
        {
            if (string.IsNullOrWhiteSpace(cid)) continue;
            var record = UnwrapEnvelope(raw);
            if (record is null) continue;

            var isContract = record["kind"]?.GetValue<string>() == "contract"
                || record["header"]?["kind"]?.GetValue<string>() == "contract";
            if (!isContract) continue;

            var formulaRoot = record["header"] ?? record;
            foreach (var slot in new[] { "pre", "post", "inv" })
            {
                if (formulaRoot?[slot] is { } formula)
                {
                    CollectCtorNames(formula, name => contractByFunction.TryAdd(name, cid));
                }
            }
        }

        return contractByFunction;
    }

    private static IEnumerable<(string Cid, JsonNode Record)> CollectMemberRecords(JsonNode envelope)
    {
        if (envelope["members"] is JsonArray membersArray)
        {
            foreach (var item in membersArray)
            {
                if (item is not null) yield return ("", DecodeEmbeddedMember(item));
            }
        }
        else if (envelope["members"] is JsonObject membersObject)
        {
            foreach (var (cid, item) in membersObject)
            {
                if (item is not null) yield return (cid, DecodeEmbeddedMember(item));
            }
        }

        if (envelope["ir"] is JsonArray ir)
        {
            foreach (var item in ir)
            {
                if (item is not null) yield return ("", item);
            }
        }
    }

    private static JsonNode DecodeEmbeddedMember(JsonNode node)
    {
        if (node is JsonObject or JsonArray) return node;
        if (node is not JsonValue value || !value.TryGetValue<string>(out var text))
            return node;

        if (TryParseJson(text, out var parsed)) return parsed;
        if (text.Length % 2 != 0 || text.Any(c => !Uri.IsHexDigit(c))) return node;

        try
        {
            var bytes = new byte[text.Length / 2];
            for (var i = 0; i < bytes.Length; i++)
            {
                bytes[i] = Convert.ToByte(text.Substring(i * 2, 2), 16);
            }

            var utf8 = Encoding.UTF8.GetString(bytes);
            return TryParseJson(utf8, out var fromHex) ? fromHex : node;
        }
        catch
        {
            return node;
        }
    }

    private static JsonObject? UnwrapEnvelope(JsonNode? node)
    {
        if (node is not JsonObject obj) return null;
        if (obj["body"] is JsonObject body && (obj["schemaVersion"] is not null || obj["header"] is not null))
        {
            return body;
        }

        return obj;
    }

    private static void CollectCtorNames(JsonNode node, Action<string> onName)
    {
        if (node is not JsonObject obj) return;
        if (obj["kind"]?.GetValue<string>() == "ctor"
            && obj["name"]?.GetValue<string>() is { } name)
        {
            onName(name);
        }

        foreach (var key in new[] { "args", "operands" })
        {
            if (obj[key] is JsonArray array)
            {
                foreach (var item in array)
                {
                    if (item is not null) CollectCtorNames(item, onName);
                }
            }
        }

        if (obj["body"] is { } body) CollectCtorNames(body, onName);
    }

    private static JsonNode? CloneOrNull(JsonNode? node) => node?.DeepClone();

    private static bool LooksLikeJson(byte[] bytes)
    {
        foreach (var b in bytes)
        {
            if (char.IsWhiteSpace((char)b)) continue;
            return b is (byte)'{' or (byte)'[';
        }

        return false;
    }

    private static bool TryParseJson(string text, out JsonNode parsed)
    {
        try
        {
            parsed = JsonNode.Parse(text) ?? new JsonObject();
            return true;
        }
        catch
        {
            parsed = new JsonObject();
            return false;
        }
    }

    private sealed class CborJsonDecoder
    {
        private readonly byte[] _bytes;
        private int _offset;

        public CborJsonDecoder(byte[] bytes)
        {
            _bytes = bytes;
        }

        public JsonNode Decode() => DecodeValue();

        private JsonNode DecodeValue()
        {
            var initial = ReadByte();
            var major = initial >> 5;
            var additional = initial & 0x1F;
            var length = ReadLength(additional);

            return major switch
            {
                0 => JsonValue.Create((long)length)!,
                2 => DecodeBytes(length),
                3 => JsonValue.Create(Encoding.UTF8.GetString(ReadBytes(length)))!,
                4 => DecodeArray(length),
                5 => DecodeMap(length),
                _ => throw new InvalidDataException($"unsupported CBOR major type {major}"),
            };
        }

        private JsonNode DecodeBytes(ulong length)
        {
            var bytes = ReadBytes(length);
            var text = Encoding.UTF8.GetString(bytes);
            if (TryParseJson(text, out var parsed)) return parsed;

            return JsonValue.Create(Convert.ToHexString(bytes).ToLowerInvariant())!;
        }

        private JsonArray DecodeArray(ulong length)
        {
            var array = new JsonArray();
            for (ulong i = 0; i < length; i++)
            {
                array.Add(DecodeValue());
            }

            return array;
        }

        private JsonObject DecodeMap(ulong length)
        {
            var obj = new JsonObject();
            for (ulong i = 0; i < length; i++)
            {
                var key = DecodeValue();
                var keyText = key is JsonValue value && value.TryGetValue<string>(out var text)
                    ? text
                    : key.ToJsonString();
                obj[keyText] = DecodeValue();
            }

            return obj;
        }

        private ulong ReadLength(int additional)
        {
            return additional switch
            {
                < 24 => (ulong)additional,
                24 => ReadByte(),
                25 => ReadUInt(2),
                26 => ReadUInt(4),
                27 => ReadUInt(8),
                _ => throw new InvalidDataException("indefinite CBOR lengths are not supported"),
            };
        }

        private ulong ReadUInt(int count)
        {
            ulong value = 0;
            for (var i = 0; i < count; i++)
            {
                value = (value << 8) | ReadByte();
            }

            return value;
        }

        private byte ReadByte()
        {
            if (_offset >= _bytes.Length) throw new EndOfStreamException("truncated CBOR");
            return _bytes[_offset++];
        }

        private byte[] ReadBytes(ulong count)
        {
            if (count > int.MaxValue) throw new InvalidDataException("CBOR item too large");
            var length = (int)count;
            if (_offset + length > _bytes.Length) throw new EndOfStreamException("truncated CBOR");
            var slice = _bytes.AsSpan(_offset, length).ToArray();
            _offset += length;
            return slice;
        }
    }
}
