// SPDX-License-Identifier: Apache-2.0

using System.Reflection.Emit;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Xml.Linq;
using Provekit.IR;
using static Provekit.IR.Predicates;
using static Provekit.IR.Terms;

namespace Provekit.Lift.CLR;

public sealed record LiftedClrDocument(
    IReadOnlyList<ContractDecl> Contracts,
    IReadOnlyList<string> Diagnostics);

public sealed record ClrInstruction(int Offset, string OpCode, string? Operand);

public static class ClrAssemblyLifter
{
    public static LiftedClrDocument LiftAssembly(string assemblyPath)
    {
        using var stream = File.OpenRead(assemblyPath);
        using var peReader = new PEReader(stream);
        if (!peReader.HasMetadata)
        {
            return new LiftedClrDocument(
                Array.Empty<ContractDecl>(),
                new[] { $"{assemblyPath}: no CLR metadata" });
        }

        var metadata = peReader.GetMetadataReader();
        var contracts = new List<ContractDecl>();

        foreach (var typeHandle in metadata.TypeDefinitions)
        {
            var type = metadata.GetTypeDefinition(typeHandle);
            var typeName = TypeName(metadata, type);
            if (typeName == "<Module>")
            {
                continue;
            }

            foreach (var methodHandle in type.GetMethods())
            {
                var method = metadata.GetMethodDefinition(methodHandle);
                if (method.RelativeVirtualAddress == 0)
                {
                    continue;
                }

                var methodName = metadata.GetString(method.Name);
                if (methodName.StartsWith('<'))
                {
                    continue;
                }

                var body = peReader.GetMethodBody(method.RelativeVirtualAddress);
                var il = body.GetILBytes() ?? Array.Empty<byte>();
                var instructions = ClrIlDecoder.Decode(il);
                var signature = Convert.ToHexString(metadata.GetBlobBytes(method.Signature)).ToLowerInvariant();
                var post = BuildMethodPost(assemblyPath, typeName, methodName, signature, body.MaxStack, instructions);
                contracts.Add(new ContractDecl(
                    $"clr:{typeName}::{methodName}#{signature}",
                    Pre: null,
                    Post: post,
                    Inv: null,
                    OutBinding: "out"));
            }
        }

        return new LiftedClrDocument(contracts, Array.Empty<string>());
    }

    public static LiftedClrDocument LiftPaths(string workspaceRoot, IReadOnlyList<string> sourcePaths)
    {
        var contractsByName = new SortedDictionary<string, ContractDecl>(StringComparer.Ordinal);
        var diagnostics = new List<string>();

        foreach (var sourcePath in sourcePaths)
        {
            var path = Path.IsPathRooted(sourcePath)
                ? sourcePath
                : Path.Combine(workspaceRoot, sourcePath);
            foreach (var assembly in EnumerateAssemblies(path))
            {
                try
                {
                    var lifted = LiftAssembly(assembly);
                    foreach (var contract in lifted.Contracts)
                    {
                        if (contractsByName.TryGetValue(contract.Name, out var existing))
                        {
                            if (Serialize.MarshalDeclarations(new[] { existing })
                                != Serialize.MarshalDeclarations(new[] { contract }))
                            {
                                diagnostics.Add($"{assembly}: duplicate CLR contract name with different body skipped: {contract.Name}");
                            }
                            continue;
                        }
                        contractsByName.Add(contract.Name, contract);
                    }
                    diagnostics.AddRange(lifted.Diagnostics);
                }
                catch (BadImageFormatException)
                {
                    diagnostics.Add($"{assembly}: not a managed CLR assembly");
                }
                catch (IOException ex)
                {
                    diagnostics.Add($"{assembly}: {ex.Message}");
                }
            }
        }

        return new LiftedClrDocument(contractsByName.Values.ToArray(), diagnostics);
    }

    private static IEnumerable<string> EnumerateAssemblies(string path)
    {
        if (File.Exists(path) && IsAssemblyPath(path))
        {
            yield return path;
            yield break;
        }
        if (!Directory.Exists(path))
        {
            yield break;
        }

        var projectOutputs = EnumerateProjectPrimaryAssemblies(path).ToArray();
        if (projectOutputs.Length > 0)
        {
            foreach (var file in projectOutputs)
            {
                yield return file;
            }
            yield break;
        }

        foreach (var file in Directory.EnumerateFiles(path, "*", SearchOption.AllDirectories)
                     .Where(IsAssemblyPath)
                     .Where(file => !PathHasSegment(file, "obj"))
                     .OrderBy(p => p, StringComparer.Ordinal))
        {
            yield return file;
        }
    }

    private static IEnumerable<string> EnumerateProjectPrimaryAssemblies(string root)
    {
        foreach (var project in Directory.EnumerateFiles(root, "*.csproj", SearchOption.AllDirectories)
                     .Where(project => !PathHasSegment(project, "bin") && !PathHasSegment(project, "obj"))
                     .OrderBy(p => p, StringComparer.Ordinal))
        {
            var projectDir = Path.GetDirectoryName(project);
            if (projectDir is null)
            {
                continue;
            }

            var assemblyName = ProjectAssemblyName(project);
            var candidates = Directory.EnumerateFiles(projectDir, $"{assemblyName}.*", SearchOption.AllDirectories)
                .Where(IsAssemblyPath)
                .Where(file => PathHasSegment(file, "bin"))
                .Where(file => !PathHasSegment(file, "ref") && !PathHasSegment(file, "refint"))
                .OrderBy(p => p, StringComparer.Ordinal)
                .ToArray();

            foreach (var candidateGroup in candidates
                         .GroupBy(Path.GetFileName, StringComparer.OrdinalIgnoreCase)
                         .OrderBy(group => group.Key, StringComparer.OrdinalIgnoreCase))
            {
                yield return SelectProjectOutputAssembly(project, candidateGroup.Key!, candidateGroup);
            }
        }
    }

    private static string SelectProjectOutputAssembly(
        string projectPath,
        string fileName,
        IEnumerable<string> candidates)
    {
        var orderedCandidates = candidates
            .OrderBy(path => path, StringComparer.Ordinal)
            .ToArray();
        var releaseCandidates = orderedCandidates
            .Where(path => PathHasBinConfiguration(path, "Release"))
            .ToArray();
        var preferredCandidates = releaseCandidates.Length > 0
            ? releaseCandidates
            : orderedCandidates;

        if (preferredCandidates.Length == 1)
        {
            return preferredCandidates[0];
        }

        var projectDir = Path.GetDirectoryName(projectPath) ?? Directory.GetCurrentDirectory();
        var relativeCandidates = preferredCandidates
            .Select(path => Path.GetRelativePath(projectDir, path))
            .OrderBy(path => path, StringComparer.Ordinal)
            .ToArray();
        var scope = releaseCandidates.Length > 0 ? "bin/Release/" : "bin/";
        throw new InvalidOperationException(
            $"{projectPath}: ambiguous CLR project output assemblies named {fileName} under {scope}: "
            + $"{string.Join(", ", relativeCandidates)}; specify the assembly path explicitly");
    }

    private static string ProjectAssemblyName(string projectPath)
    {
        try
        {
            var doc = XDocument.Load(projectPath);
            var explicitName = doc.Descendants()
                .FirstOrDefault(element => element.Name.LocalName == "AssemblyName")
                ?.Value
                .Trim();
            if (!string.IsNullOrEmpty(explicitName))
            {
                return explicitName;
            }
        }
        catch
        {
            // Non-SDK or generated project files still default to the file stem.
        }

        return Path.GetFileNameWithoutExtension(projectPath);
    }

    private static bool PathHasSegment(string path, string segment)
    {
        return path.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)
            .Any(part => part.Equals(segment, StringComparison.OrdinalIgnoreCase));
    }

    private static bool PathHasBinConfiguration(string path, string configuration)
    {
        var parts = path.Split(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        for (var index = 0; index < parts.Length - 1; index++)
        {
            if (parts[index].Equals("bin", StringComparison.OrdinalIgnoreCase)
                && parts[index + 1].Equals(configuration, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    private static bool IsAssemblyPath(string path)
    {
        var ext = Path.GetExtension(path);
        return ext.Equals(".dll", StringComparison.OrdinalIgnoreCase)
               || ext.Equals(".exe", StringComparison.OrdinalIgnoreCase);
    }

    private static string TypeName(MetadataReader metadata, TypeDefinition type)
    {
        var name = metadata.GetString(type.Name);
        var declaringType = type.GetDeclaringType();
        if (!declaringType.IsNil)
        {
            return $"{TypeName(metadata, metadata.GetTypeDefinition(declaringType))}+{name}";
        }

        var ns = metadata.GetString(type.Namespace);
        return string.IsNullOrEmpty(ns) ? name : $"{ns}.{name}";
    }

    private static Formula BuildMethodPost(
        string assemblyPath,
        string typeName,
        string methodName,
        string signatureHex,
        int maxStack,
        IReadOnlyList<ClrInstruction> instructions)
    {
        var instructionTerms = instructions
            .Select(instruction =>
                instruction.Operand is null
                    ? Ctor("clr:op", Num(instruction.Offset), StrConst(instruction.OpCode))
                    : Ctor("clr:op", Num(instruction.Offset), StrConst(instruction.OpCode), StrConst(instruction.Operand)))
            .ToArray();

        return Atomic(
            "clr:method-body",
            StrConst(Path.GetFileName(assemblyPath)),
            StrConst(typeName),
            StrConst(methodName),
            Ctor("clr:signature", StrConst(signatureHex)),
            Ctor("clr:maxstack", Num(maxStack)),
            Ctor("clr:instructions", instructionTerms));
    }
}

internal static class ClrIlDecoder
{
    private static readonly IReadOnlyDictionary<ushort, OpCode> OpCodesByValue = BuildOpcodeMap();

    public static IReadOnlyList<ClrInstruction> Decode(byte[] il)
    {
        var instructions = new List<ClrInstruction>();
        var offset = 0;
        while (offset < il.Length)
        {
            var instructionOffset = offset;
            var key = (ushort)il[offset++];
            if (key == 0xfe && offset < il.Length)
            {
                key = (ushort)(0xfe00 | il[offset++]);
            }

            if (!OpCodesByValue.TryGetValue(key, out var opCode))
            {
                instructions.Add(new ClrInstruction(instructionOffset, $"unknown:0x{key:x}", null));
                continue;
            }

            var operand = ReadOperand(il, ref offset, instructionOffset, opCode);
            instructions.Add(new ClrInstruction(instructionOffset, opCode.Name ?? $"op:0x{key:x}", operand));
        }

        return instructions;
    }

    private static IReadOnlyDictionary<ushort, OpCode> BuildOpcodeMap()
    {
        return typeof(OpCodes)
            .GetFields(System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Static)
            .Where(field => field.FieldType == typeof(OpCode))
            .Select(field => (OpCode)field.GetValue(null)!)
            .ToDictionary(op => unchecked((ushort)op.Value), op => op);
    }

    private static string? ReadOperand(byte[] il, ref int offset, int instructionOffset, OpCode opCode)
    {
        switch (opCode.OperandType)
        {
            case OperandType.InlineNone:
                return null;
            case OperandType.ShortInlineI:
                return ((sbyte)il[offset++]).ToString(System.Globalization.CultureInfo.InvariantCulture);
            case OperandType.InlineI:
                return ReadInt32(il, ref offset).ToString(System.Globalization.CultureInfo.InvariantCulture);
            case OperandType.InlineI8:
                return ReadInt64(il, ref offset).ToString(System.Globalization.CultureInfo.InvariantCulture);
            case OperandType.ShortInlineR:
                return ReadSingle(il, ref offset).ToString("R", System.Globalization.CultureInfo.InvariantCulture);
            case OperandType.InlineR:
                return ReadDouble(il, ref offset).ToString("R", System.Globalization.CultureInfo.InvariantCulture);
            case OperandType.ShortInlineVar:
                return $"V_{il[offset++]}";
            case OperandType.InlineVar:
                return $"V_{ReadUInt16(il, ref offset)}";
            case OperandType.ShortInlineBrTarget:
                {
                    var delta = (sbyte)il[offset++];
                    return $"IL_{offset + delta:x4}";
                }
            case OperandType.InlineBrTarget:
                {
                    var delta = ReadInt32(il, ref offset);
                    return $"IL_{offset + delta:x4}";
                }
            case OperandType.InlineSwitch:
                {
                    var count = ReadInt32(il, ref offset);
                    var deltas = new int[count];
                    for (var i = 0; i < count; i++)
                    {
                        deltas[i] = ReadInt32(il, ref offset);
                    }
                    var baseOffset = offset;
                    return string.Join(",", deltas.Select(delta => $"IL_{baseOffset + delta:x4}"));
                }
            case OperandType.InlineString:
            case OperandType.InlineField:
            case OperandType.InlineMethod:
            case OperandType.InlineSig:
            case OperandType.InlineTok:
            case OperandType.InlineType:
                return $"token:0x{ReadUInt32(il, ref offset):x8}";
            default:
                throw new InvalidOperationException(
                    $"unsupported CLR operand type {opCode.OperandType} at IL_{instructionOffset:x4}");
        }
    }

    private static ushort ReadUInt16(byte[] bytes, ref int offset)
    {
        var value = BitConverter.ToUInt16(bytes, offset);
        offset += 2;
        return value;
    }

    private static uint ReadUInt32(byte[] bytes, ref int offset)
    {
        var value = BitConverter.ToUInt32(bytes, offset);
        offset += 4;
        return value;
    }

    private static int ReadInt32(byte[] bytes, ref int offset)
    {
        var value = BitConverter.ToInt32(bytes, offset);
        offset += 4;
        return value;
    }

    private static long ReadInt64(byte[] bytes, ref int offset)
    {
        var value = BitConverter.ToInt64(bytes, offset);
        offset += 8;
        return value;
    }

    private static float ReadSingle(byte[] bytes, ref int offset)
    {
        var value = BitConverter.ToSingle(bytes, offset);
        offset += 4;
        return value;
    }

    private static double ReadDouble(byte[] bytes, ref int offset)
    {
        var value = BitConverter.ToDouble(bytes, offset);
        offset += 8;
        return value;
    }
}
