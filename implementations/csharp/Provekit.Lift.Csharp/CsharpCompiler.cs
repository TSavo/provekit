using System.Text.Json;
using System.Text.Json.Nodes;

namespace Provekit.Lift.Csharp;

public class CsharpCompiler
{
    private int _indent;
    private readonly List<string> _lines = new();

    public string Compile(JsonNode? ir)
    {
        _lines.Clear();
        _indent = 0;
        EmitStmt(ir);
        var body = string.Join("\n", _lines);
        return WrapFunction(body);
    }

    public string CompileBody(JsonNode? ir)
    {
        _lines.Clear();
        _indent = 0;
        EmitStmt(ir);
        return string.Join("\n", _lines);
    }

    private void EmitStmt(JsonNode? node)
    {
        if (node is null) return;
        var kind = node["kind"]?.GetValue<string>();
        if (kind != "ctor" && kind != "op")
        {
            var expr = EmitExpr(node);
            _lines.Add($"{Indent()}({expr});");
            return;
        }

        var name = node["name"]?.GetValue<string>() ?? "";
        var args = node["args"]?.AsArray();

        switch (name)
        {
            case "csharp:seq":
                EmitSeq(args);
                break;
            case "csharp:if":
                EmitIf(args);
                break;
            case "csharp:while":
                EmitWhile(args);
                break;
            case "csharp:for":
                EmitFor(args);
                break;
            case "csharp:foreach":
                EmitForEach(args);
                break;
            case "csharp:return":
                EmitReturn(args);
                break;
            case "csharp:throw":
                EmitThrow(args);
                break;
            case "csharp:break":
                _lines.Add($"{Indent()}break;");
                break;
            case "csharp:continue":
                _lines.Add($"{Indent()}continue;");
                break;
            case "csharp:skip":
                _lines.Add($"{Indent()};");
                break;
            case "csharp:decl":
                EmitDecl(args);
                break;
            case "csharp:assign":
                EmitAssign(args);
                break;
            case "csharp:call":
            {
                var expr = EmitCallExpr(args);
                _lines.Add($"{Indent()}{expr};");
                break;
            }
            default:
                if (name.StartsWith("csharp:"))
                {
                    var expr = EmitCtor(node);
                    _lines.Add($"{Indent()}({expr});");
                }
                else
                {
                    _lines.Add($"{Indent()}/* unhandled: {name} */;");
                }
                break;
        }
    }

    private void EmitSeq(JsonArray? args)
    {
        if (args is null) return;
        foreach (var arg in args)
            EmitStmt(arg);
    }

    private void EmitIf(JsonArray? args)
    {
        if (args is null || args.Count < 3) return;
        var cond = EmitCondition(args[0]);
        _lines.Add($"{Indent()}if ({cond})");
        _lines.Add($"{Indent()}{{");
        _indent++;
        EmitStmt(args[1]);
        _indent--;
        _lines.Add($"{Indent()}}}");
        _lines.Add($"{Indent()}else");
        _lines.Add($"{Indent()}{{");
        _indent++;
        EmitStmt(args[2]);
        _indent--;
        _lines.Add($"{Indent()}}}");
    }

    private void EmitWhile(JsonArray? args)
    {
        if (args is null || args.Count < 2) return;
        var cond = EmitCondition(args[0]);
        _lines.Add($"{Indent()}while ({cond})");
        _lines.Add($"{Indent()}{{");
        _indent++;
        EmitStmt(args[1]);
        _indent--;
        _lines.Add($"{Indent()}}}");
    }

    private void EmitFor(JsonArray? args)
    {
        if (args is null || args.Count < 4) return;
        var init = EmitExpr(args[0]);
        var cond = EmitCondition(args[1]);
        var update = EmitExpr(args[2]);
        _lines.Add($"{Indent()}for ({init}; {cond}; {update})");
        _lines.Add($"{Indent()}{{");
        _indent++;
        EmitStmt(args[3]);
        _indent--;
        _lines.Add($"{Indent()}}}");
    }

    private void EmitForEach(JsonArray? args)
    {
        if (args is null || args.Count < 3) return;
        var varName = args[0]?["value"]?.GetValue<string>() ?? "x";
        var collection = EmitExpr(args[1]);
        _lines.Add($"{Indent()}foreach (var {varName} in {collection})");
        _lines.Add($"{Indent()}{{");
        _indent++;
        EmitStmt(args[2]);
        _indent--;
        _lines.Add($"{Indent()}}}");
    }

    private void EmitReturn(JsonArray? args)
    {
        if (args is null || args.Count < 1) return;
        var expr = EmitExpr(args[0]);
        _lines.Add($"{Indent()}return ({expr});");
    }

    private void EmitThrow(JsonArray? args)
    {
        if (args is null || args.Count < 1)
        {
            _lines.Add($"{Indent()}throw new System.Exception();");
            return;
        }
        var expr = EmitExpr(args[0]);
        _lines.Add($"{Indent()}throw ({expr});");
    }

    private void EmitDecl(JsonArray? args)
    {
        if (args is null || args.Count < 2) return;
        var name = EmitStringValue(args[0]);
        var expr = EmitExpr(args[1]);
        _lines.Add($"{Indent()}int {name} = {expr};");
    }

    private void EmitAssign(JsonArray? args)
    {
        if (args is null || args.Count < 2) return;
        var target = EmitLValue(args[0]);
        var value = EmitExpr(args[1]);
        _lines.Add($"{Indent()}{target} = {value};");
    }

    private string EmitCondition(JsonNode? node)
    {
        if (node is null) return "true";
        var kind = node["kind"]?.GetValue<string>();
        var name = node["name"]?.GetValue<string>() ?? "";

        if (kind == "const")
        {
            var v = node["value"];
            if (v?.GetValue<bool>() == false) return "false";
            if (v?.GetValue<bool>() == true) return "true";
            if (v?.GetValue<int>() is int vi) return vi != 0 ? "true" : "false";
            return $"({EmitExpr(node)})";
        }

        return $"({EmitExpr(node)})";
    }

    private string EmitExpr(JsonNode? node)
    {
        if (node is null) return "0";
        var kind = node["kind"]?.GetValue<string>();

        switch (kind)
        {
            case "const":
            {
                var value = node["value"];
                if (value is JsonValue jv)
                {
                    if (jv.TryGetValue<int>(out var i)) return i.ToString();
                    if (jv.TryGetValue<bool>(out var b)) return b ? "true" : "false";
                    if (jv.TryGetValue<string>(out var s)) return $"\"{Escape(s)}\"";
                }
                return value?.ToString() ?? "0";
            }

            case "var":
                return node["name"]?.GetValue<string>() ?? "x";

            case "ctor":
                return EmitCtor(node);

            default:
                return "0";
        }
    }

    private string EmitCtor(JsonNode? node)
    {
        if (node is null) return "0";
        var name = node["name"]?.GetValue<string>() ?? "";
        var args = node["args"]?.AsArray();

        switch (name)
        {
            case "csharp:add": return $"({EmitExpr(args?[0])} + {EmitExpr(args?[1])})";
            case "csharp:sub": return $"({EmitExpr(args?[0])} - {EmitExpr(args?[1])})";
            case "csharp:mul": return $"({EmitExpr(args?[0])} * {EmitExpr(args?[1])})";
            case "csharp:div": return $"({EmitExpr(args?[0])} / {EmitExpr(args?[1])})";
            case "csharp:mod": return $"({EmitExpr(args?[0])} % {EmitExpr(args?[1])})";
            case "csharp:neg": return $"(-{EmitExpr(args?[0])})";
            case "csharp:not": return $"(!{EmitExpr(args?[0])})";
            case "csharp:eq": return $"({EmitExpr(args?[0])} == {EmitExpr(args?[1])})";
            case "csharp:ne": return $"({EmitExpr(args?[0])} != {EmitExpr(args?[1])})";
            case "csharp:lt": return $"({EmitExpr(args?[0])} < {EmitExpr(args?[1])})";
            case "csharp:le": return $"({EmitExpr(args?[0])} <= {EmitExpr(args?[1])})";
            case "csharp:gt": return $"({EmitExpr(args?[0])} > {EmitExpr(args?[1])})";
            case "csharp:ge": return $"({EmitExpr(args?[0])} >= {EmitExpr(args?[1])})";
            case "csharp:and": return $"({EmitExpr(args?[0])} && {EmitExpr(args?[1])})";
            case "csharp:or": return $"({EmitExpr(args?[0])} || {EmitExpr(args?[1])})";
            case "csharp:bitand": return $"({EmitExpr(args?[0])} & {EmitExpr(args?[1])})";
            case "csharp:bitor": return $"({EmitExpr(args?[0])} | {EmitExpr(args?[1])})";
            case "csharp:bitxor": return $"({EmitExpr(args?[0])} ^ {EmitExpr(args?[1])})";
            case "csharp:shl": return $"({EmitExpr(args?[0])} << {EmitExpr(args?[1])})";
            case "csharp:shr": return $"({EmitExpr(args?[0])} >> {EmitExpr(args?[1])})";
            case "csharp:bitnot": return $"(~{EmitExpr(args?[0])})";
            case "csharp:ite":
                return $"({EmitExpr(args?[0])} ? {EmitExpr(args?[1])} : {EmitExpr(args?[2])})";
            case "csharp:member":
                return $"{EmitExpr(args?[0])}.{EmitStringValue(args?[1])}";
            case "csharp:index":
                return $"{EmitExpr(args?[0])}[{EmitExpr(args?[1])}]";
            case "csharp:cast":
                return $"({EmitStringValue(args?[0])})({EmitExpr(args?[1])})";
            case "csharp:new":
                return EmitNewExpr(args);
            case "csharp:call":
                return EmitCallExpr(args);
            case "csharp:preinc":
                return $"(++{EmitExpr(args?[0])})";
            case "csharp:predec":
                return $"(--{EmitExpr(args?[0])})";
            case "csharp:postinc":
                return $"({EmitExpr(args?[0])}++)";
            case "csharp:postdec":
                return $"({EmitExpr(args?[0])}--)";
            case "csharp:assign":
                return $"({EmitLValue(args?[0])} = {EmitExpr(args?[1])})";
            case "csharp:decl":
            {
                var n = EmitStringValue(args?[0]);
                var e = EmitExpr(args?[1]);
                return $"int {n} = {e}";
            }
            case "csharp:skip":
                return "";
            case "csharp:return":
                return $"return ({EmitExpr(args?[0])})";
            case "csharp:throw":
                return $"throw ({EmitExpr(args?[0])})";
            case "csharp:while":
            case "csharp:for":
            case "csharp:foreach":
            case "csharp:seq":
            case "csharp:if":
            case "csharp:break":
            case "csharp:continue":
                return "";

            default:
                return $"{name}({string.Join(", ", args?.Select(a => EmitExpr(a)) ?? Enumerable.Empty<string>())})";
        }
    }

    private string EmitCallExpr(JsonArray? args)
    {
        if (args is null || args.Count < 1) return "unknown()";
        var target = EmitStringValue(args[0]);
        var callArgs = args.Skip(1).Select(a => EmitExpr(a)).ToList();
        return $"{target}({string.Join(", ", callArgs)})";
    }

    private string EmitNewExpr(JsonArray? args)
    {
        if (args is null || args.Count < 1) return "new object()";
        var typeName = EmitStringValue(args[0]);
        var ctorArgs = args.Skip(1).Select(a => EmitExpr(a)).ToList();
        return $"new {typeName}({string.Join(", ", ctorArgs)})";
    }

    private string EmitLValue(JsonNode? node)
    {
        if (node is null) return "x";
        var kind = node["kind"]?.GetValue<string>();
        if (kind == "var") return node["name"]?.GetValue<string>() ?? "x";
        return EmitExpr(node);
    }

    private string EmitStringValue(JsonNode? node)
    {
        if (node is null) return "";
        var kind = node["kind"]?.GetValue<string>();
        if (kind == "const") return node["value"]?.GetValue<string>() ?? "";
        return EmitExpr(node);
    }

    private static string Escape(string s) => s.Replace("\\", "\\\\").Replace("\"", "\\\"");

    private string WrapFunction(string body)
    {
        return $@"static int F({string.Join(", ", Enumerable.Range(0, 2).Select(i => $"int x{i}"))})
{{{body}
}}";
    }

    private string Indent() => new(' ', _indent * 4);
}
