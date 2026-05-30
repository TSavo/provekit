const std = @import("std");
const provekit = @import("provekit-ir");

const Ast = std.zig.Ast;
const Node = Ast.Node;
const Token = std.zig.Token;

pub const VERSION = "0.1.0-draft";
pub const DIALECT = "zig-source";

const LiftError = error{Refused};

pub const Refusal = struct {
    kind: []const u8,
    function: ?[]const u8,
    line: usize,
    reason: []const u8,

    pub fn jsonStringify(self: Refusal, jws: anytype) !void {
        try jws.beginObject();
        try jws.objectField("function");
        if (self.function) |function| try jws.write(function) else try jws.write(null);
        try jws.objectField("kind");
        try jws.write(self.kind);
        try jws.objectField("line");
        try jws.write(self.line);
        try jws.objectField("reason");
        try jws.write(self.reason);
        try jws.endObject();
    }
};

pub const LiftOutput = struct {
    declarations: []FunctionContract,
    refusals: []Refusal,
};

const Locus = struct {
    file: []const u8,
    line: usize,
    col: usize,

    pub fn jsonStringify(self: Locus, jws: anytype) !void {
        try jws.beginObject();
        try jws.objectField("col");
        try jws.write(self.col);
        try jws.objectField("file");
        try jws.write(self.file);
        try jws.objectField("line");
        try jws.write(self.line);
        try jws.endObject();
    }
};

pub const Effect = union(enum) {
    reads: []const u8,
    writes: []const u8,
    io,
    unsafe,
    panics,
    unresolved_call: []const u8,
    opaque_loop: []const u8,

    fn rank(self: Effect) u8 {
        return switch (self) {
            .reads => 0,
            .writes => 1,
            .io => 2,
            .unsafe => 3,
            .panics => 4,
            .unresolved_call => 5,
            .opaque_loop => 6,
        };
    }

    fn secondary(self: Effect) []const u8 {
        return switch (self) {
            .reads => |target| target,
            .writes => |target| target,
            .unresolved_call => |name| name,
            .opaque_loop => |cid| cid,
            else => "",
        };
    }

    fn eql(a: Effect, b: Effect) bool {
        if (a.rank() != b.rank()) return false;
        return std.mem.eql(u8, a.secondary(), b.secondary());
    }

    fn lessThan(_: void, a: Effect, b: Effect) bool {
        if (a.rank() != b.rank()) return a.rank() < b.rank();
        return std.mem.lessThan(u8, a.secondary(), b.secondary());
    }

    pub fn jsonStringify(self: Effect, jws: anytype) !void {
        try jws.beginObject();
        switch (self) {
            .reads => |target| {
                try jws.objectField("kind");
                try jws.write("reads");
                try jws.objectField("target");
                try jws.write(target);
            },
            .writes => |target| {
                try jws.objectField("kind");
                try jws.write("writes");
                try jws.objectField("target");
                try jws.write(target);
            },
            .io => {
                try jws.objectField("kind");
                try jws.write("io");
            },
            .unsafe => {
                try jws.objectField("kind");
                try jws.write("unsafe");
            },
            .panics => {
                try jws.objectField("kind");
                try jws.write("panics");
            },
            .unresolved_call => |name| {
                try jws.objectField("kind");
                try jws.write("unresolved_call");
                try jws.objectField("name");
                try jws.write(name);
            },
            .opaque_loop => |loop_cid| {
                try jws.objectField("kind");
                try jws.write("opaque_loop");
                try jws.objectField("loopCid");
                try jws.write(loop_cid);
            },
        }
        try jws.endObject();
    }
};

pub const FunctionContract = struct {
    fn_name: []const u8,
    bridge_source_symbol: ?[]const u8 = null,
    formals: []const []const u8,
    formal_sorts: []const provekit.Sort,
    return_sort: provekit.Sort,
    pre: provekit.Formula,
    post: provekit.Formula,
    body_cid: ?[]const u8,
    effects: []const Effect,
    locus: Locus,
    body_term: provekit.Term,

    pub fn bodyTerm(self: FunctionContract) provekit.Term {
        return self.body_term;
    }

    pub fn jsonStringify(self: FunctionContract, jws: anytype) !void {
        try jws.beginObject();
        try jws.objectField("autoMintedMementos");
        try jws.beginArray();
        try jws.endArray();
        try jws.objectField("bodyCid");
        if (self.body_cid) |cid| try jws.write(cid) else try jws.write(null);
        if (self.bridge_source_symbol) |symbol| {
            try jws.objectField("bridgeSourceSymbol");
            try jws.write(symbol);
        }
        try jws.objectField("effects");
        try jws.write(self.effects);
        try jws.objectField("fnName");
        try jws.write(self.fn_name);
        try jws.objectField("formalSorts");
        try jws.write(self.formal_sorts);
        try jws.objectField("formals");
        try jws.write(self.formals);
        try jws.objectField("kind");
        try jws.write("function-contract");
        try jws.objectField("locus");
        try jws.write(self.locus);
        try jws.objectField("post");
        try jws.write(self.post);
        try jws.objectField("pre");
        try jws.write(self.pre);
        try jws.objectField("returnSort");
        try jws.write(self.return_sort);
        try jws.objectField("schemaVersion");
        try jws.write("1");
        try jws.endObject();
    }
};

pub const SourceSpan = struct {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
};

pub const SugarAstTemplate = union(enum) {
    block: struct { stmts: []const SugarAstTemplate },
    let: struct { pat: *const SugarAstTemplate, init: ?*const SugarAstTemplate },
    expr_stmt: struct { expr: *const SugarAstTemplate, trailing_semi: bool },
    call: struct { func: *const SugarAstTemplate, args: []const SugarAstTemplate },
    method_call: struct { receiver: *const SugarAstTemplate, method: []const u8, args: []const SugarAstTemplate },
    ident: []const u8,
    param_ref: usize,
    path: []const []const u8,
    lit: struct { ty: []const u8, value: []const u8 },
    ref: struct { mutability: bool, expr: *const SugarAstTemplate },
    @"return": ?*const SugarAstTemplate,
    tuple: struct { elems: []const SugarAstTemplate },
    array: struct { elems: []const SugarAstTemplate },
    binary: struct { op: []const u8, left: *const SugarAstTemplate, right: *const SugarAstTemplate },
    binding: []const u8,
    wildcard,
    pat_tuple: struct { elems: []const SugarAstTemplate },
    other: []const u8,

    pub fn jsonStringify(self: SugarAstTemplate, jws: anytype) !void {
        try jws.beginObject();
        switch (self) {
            .block => |v| {
                try jws.objectField("kind");
                try jws.write("block");
                try jws.objectField("stmts");
                try jws.write(v.stmts);
            },
            .let => |v| {
                try jws.objectField("kind");
                try jws.write("let");
                try jws.objectField("pat");
                try jws.write(v.pat.*);
                try jws.objectField("init");
                if (v.init) |init| try jws.write(init.*) else try jws.write(null);
            },
            .expr_stmt => |v| {
                try jws.objectField("kind");
                try jws.write("expr_stmt");
                try jws.objectField("expr");
                try jws.write(v.expr.*);
                try jws.objectField("trailing_semi");
                try jws.write(v.trailing_semi);
            },
            .call => |v| {
                try jws.objectField("kind");
                try jws.write("call");
                try jws.objectField("func");
                try jws.write(v.func.*);
                try jws.objectField("args");
                try jws.write(v.args);
            },
            .method_call => |v| {
                try jws.objectField("kind");
                try jws.write("method_call");
                try jws.objectField("receiver");
                try jws.write(v.receiver.*);
                try jws.objectField("method");
                try jws.write(v.method);
                try jws.objectField("args");
                try jws.write(v.args);
            },
            .ident => |name| {
                try jws.objectField("kind");
                try jws.write("ident");
                try jws.objectField("name");
                try jws.write(name);
            },
            .param_ref => |index| {
                try jws.objectField("kind");
                try jws.write("param_ref");
                try jws.objectField("index");
                try jws.write(index);
            },
            .path => |segments| {
                try jws.objectField("kind");
                try jws.write("path");
                try jws.objectField("segments");
                try jws.write(segments);
            },
            .lit => |v| {
                try jws.objectField("kind");
                try jws.write("lit");
                try jws.objectField("ty");
                try jws.write(v.ty);
                try jws.objectField("value");
                try jws.write(v.value);
            },
            .ref => |v| {
                try jws.objectField("kind");
                try jws.write("ref");
                try jws.objectField("mutability");
                try jws.write(v.mutability);
                try jws.objectField("expr");
                try jws.write(v.expr.*);
            },
            .@"return" => |maybe_expr| {
                try jws.objectField("kind");
                try jws.write("return");
                try jws.objectField("expr");
                if (maybe_expr) |expr| try jws.write(expr.*) else try jws.write(null);
            },
            .tuple => |v| {
                try jws.objectField("kind");
                try jws.write("tuple");
                try jws.objectField("elems");
                try jws.write(v.elems);
            },
            .array => |v| {
                try jws.objectField("kind");
                try jws.write("array");
                try jws.objectField("elems");
                try jws.write(v.elems);
            },
            .binary => |v| {
                try jws.objectField("kind");
                try jws.write("binary");
                try jws.objectField("op");
                try jws.write(v.op);
                try jws.objectField("left");
                try jws.write(v.left.*);
                try jws.objectField("right");
                try jws.write(v.right.*);
            },
            .binding => |name| {
                try jws.objectField("kind");
                try jws.write("binding");
                try jws.objectField("name");
                try jws.write(name);
            },
            .wildcard => {
                try jws.objectField("kind");
                try jws.write("wildcard");
            },
            .pat_tuple => |v| {
                try jws.objectField("kind");
                try jws.write("pat_tuple");
                try jws.objectField("elems");
                try jws.write(v.elems);
            },
            .other => |variant| {
                try jws.objectField("kind");
                try jws.write("other");
                try jws.objectField("variant");
                try jws.write(variant);
            },
        }
        try jws.endObject();
    }
};

pub const SugarBodySource = struct {
    file: []const u8,
    span: SourceSpan,
    source_cid: []const u8,
    body_text: []const u8,
    ast_template: SugarAstTemplate,
    template_cid: []const u8,
    param_names: []const []const u8,
};

pub const BindingTemplate = struct {
    concept_name: ?[]const u8 = null,
    library_tag: ?[]const u8 = null,
    target_library_tag: ?[]const u8 = null,
    family: ?[]const u8 = null,
    ast_template: ?SugarAstTemplate = null,
    template_cid: []const u8,
    param_names: []const []const u8 = &.{},
    contract_cid: ?[]const u8 = null,
};

pub const ParamBinding = struct {
    index: usize,
    source_text: []const u8,
};

pub const RecognizeTag = struct {
    file: []const u8,
    span: SourceSpan,
    function_name: []const u8,
    concept_name: ?[]const u8,
    library_tag: ?[]const u8,
    family: ?[]const u8,
    template_cid: []const u8,
    contract_cid: ?[]const u8,
    match_tier: []const u8,
    param_bindings: []const ParamBinding,
};

pub const RecognizeResponse = struct {
    tags: []const RecognizeTag,
};

const Global = struct {
    name: []const u8,
    mutable: bool,
};

const FunctionInfo = struct {
    node: Node.Index,
    namespace: []const []const u8,
};

const Lifter = struct {
    alloc: std.mem.Allocator,
    tree: *const Ast,
    source: []const u8,
    path: []const u8,
    globals: std.ArrayList(Global),
    functions: std.ArrayList(FunctionInfo),
    known_functions: std.ArrayList([]const u8),
    declarations: std.ArrayList(FunctionContract),
    refusals: std.ArrayList(Refusal),
    current_function: ?[]const u8 = null,
    current_locals: std.ArrayList([]const u8),
    current_effects: std.ArrayList(Effect),

    fn init(alloc: std.mem.Allocator, tree: *const Ast, source: []const u8, path: []const u8) Lifter {
        return .{
            .alloc = alloc,
            .tree = tree,
            .source = source,
            .path = path,
            .globals = .empty,
            .functions = .empty,
            .known_functions = .empty,
            .declarations = .empty,
            .refusals = .empty,
            .current_locals = .empty,
            .current_effects = .empty,
        };
    }

    fn lift(self: *Lifter) !LiftOutput {
        const roots = self.tree.rootDecls();
        for (roots) |decl| try self.collectTopLevel(decl, &.{});
        for (self.functions.items) |function| {
            self.emitFunction(function) catch |err| switch (err) {
                error.Refused => {},
                else => return err,
            };
        }

        if (self.declarations.items.len > 0) {
            const source_unit = try self.sourceUnitContract();
            try self.declarations.insert(self.alloc, 0, source_unit);
        }

        return .{
            .declarations = try self.declarations.toOwnedSlice(self.alloc),
            .refusals = try self.refusals.toOwnedSlice(self.alloc),
        };
    }

    fn collectTopLevel(self: *Lifter, node: Node.Index, namespace: []const []const u8) anyerror!void {
        switch (self.tree.nodeTag(node)) {
            .fn_decl => try self.collectFunction(node, namespace),
            .global_var_decl, .simple_var_decl, .aligned_var_decl => try self.collectGlobalOrNamespace(node, namespace),
            .test_decl => {},
            .container_field_init, .container_field_align, .container_field => {},
            else => {},
        }
    }

    fn collectFunction(self: *Lifter, node: Node.Index, namespace: []const []const u8) anyerror!void {
        var buffer: [1]Node.Index = undefined;
        const proto_node = self.tree.nodeData(node).node_and_node[0];
        const proto = self.tree.fullFnProto(&buffer, proto_node) orelse return;
        const name_token = proto.name_token orelse return;
        const name = self.tree.tokenSlice(name_token);
        const qualified = try self.qualifiedName(namespace, name);
        try self.functions.append(self.alloc, .{ .node = node, .namespace = try cloneStringSlice(self.alloc, namespace) });
        try appendUniqueString(self.alloc, &self.known_functions, name);
        try appendUniqueString(self.alloc, &self.known_functions, qualified);
    }

    fn collectGlobalOrNamespace(self: *Lifter, node: Node.Index, namespace: []const []const u8) anyerror!void {
        const var_decl = self.fullVarDecl(node) orelse return;
        const name = self.varDeclName(var_decl) orelse return;
        if (var_decl.ast.init_node.unwrap()) |init_node| {
            if (self.isContainerDecl(init_node)) {
                var next_ns = std.ArrayList([]const u8).empty;
                for (namespace) |part| try next_ns.append(self.alloc, part);
                try next_ns.append(self.alloc, name);
                const members = self.containerMembers(init_node) orelse return;
                for (members) |member| try self.collectTopLevel(member, next_ns.items);
                return;
            }
        }
        try appendGlobal(self.alloc, &self.globals, .{ .name = name, .mutable = isMutToken(self.tree.tokenTag(var_decl.ast.mut_token)) });
    }

    fn emitFunction(self: *Lifter, info: FunctionInfo) !void {
        self.current_locals.clearRetainingCapacity();
        self.current_effects.clearRetainingCapacity();

        const data = self.tree.nodeData(info.node).node_and_node;
        const proto_node = data[0];
        const body_node = data[1];
        var buffer: [1]Node.Index = undefined;
        const proto = self.tree.fullFnProto(&buffer, proto_node) orelse return self.refuse(info.node, "unhandled-syntax", "expected function prototype");
        const short_name = if (proto.name_token) |tok| self.tree.tokenSlice(tok) else return self.refuse(info.node, "unhandled-syntax", "anonymous function declaration");
        const qualified = try self.qualifiedName(info.namespace, short_name);
        self.current_function = qualified;
        defer self.current_function = null;

        if (proto.ast.align_expr != .none or proto.ast.addrspace_expr != .none or proto.ast.section_expr != .none or proto.ast.callconv_expr != .none) {
            return self.refuse(proto_node, "unsupported-function-qualifier", "align, addrspace, linksection, and callconv are out of scope for AST source lifting");
        }

        const return_type_node = proto.ast.return_type.unwrap() orelse return self.refuse(proto_node, "unsupported-return-sort", "missing return type");
        const return_sort = try self.sortFromType(return_type_node, true);

        var formals = std.ArrayList([]const u8).empty;
        var formal_sorts = std.ArrayList(provekit.Sort).empty;
        var it = proto.iterate(self.tree);
        while (it.next()) |param| {
            if (param.anytype_ellipsis3 != null) return self.refuse(proto_node, "unsupported-generic", "anytype and varargs parameters require comptime/Sema");
            if (param.comptime_noalias) |tok| {
                if (self.tree.tokenTag(tok) == .keyword_comptime) return self.refuse(proto_node, "unsupported-generic", "comptime parameters require Zig Sema");
            }
            const name_token = param.name_token orelse return self.refuse(proto_node, "unsupported-parameter", "unnamed parameters are not lifted");
            const type_node = param.type_expr orelse return self.refuse(proto_node, "unsupported-parameter", "parameter type missing from AST");
            const name = self.tree.tokenSlice(name_token);
            try formals.append(self.alloc, name);
            try formal_sorts.append(self.alloc, try self.sortFromType(type_node, false));
            try appendUniqueString(self.alloc, &self.current_locals, name);
        }

        const body_term = try self.emitBlock(body_node);
        const effects = try self.current_effects.toOwnedSlice(self.alloc);
        std.mem.sort(Effect, effects, {}, Effect.lessThan);

        const post_terms = try termArgs(self.alloc, .{ provekit.Var("return_value"), body_term });
        const contract = FunctionContract{
            .fn_name = qualified,
            .formals = try formals.toOwnedSlice(self.alloc),
            .formal_sorts = try formal_sorts.toOwnedSlice(self.alloc),
            .return_sort = return_sort,
            .pre = trueFormula(),
            .post = provekit.Atomic("=", post_terms),
            .body_cid = null,
            .effects = effects,
            .locus = self.locusOf(info.node),
            .body_term = body_term,
        };
        try self.declarations.append(self.alloc, contract);
    }

    fn emitBlock(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        var buf: [2]Node.Index = undefined;
        const stmts = self.tree.blockStatements(&buf, node) orelse return self.refuse(node, "unhandled-syntax", "expected a block body");
        return self.emitSeq(stmts);
    }

    fn emitSeq(self: *Lifter, stmts: []const Node.Index) anyerror!provekit.Term {
        var result: ?provekit.Term = null;
        for (stmts) |stmt| {
            const term = try self.emitStmt(stmt);
            result = if (result) |prev| try self.ctor("zig:seq", .{ prev, term }) else term;
        }
        return result orelse try self.skipTerm();
    }

    fn emitStmt(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        return switch (self.tree.nodeTag(node)) {
            .block, .block_semicolon, .block_two, .block_two_semicolon => self.emitBlock(node),
            .local_var_decl, .simple_var_decl, .aligned_var_decl => self.emitLocalDecl(node),
            .assign => self.emitAssign(node),
            .assign_add, .assign_sub, .assign_mul, .assign_div, .assign_mod, .assign_shl, .assign_shr, .assign_bit_and, .assign_bit_or, .assign_bit_xor => self.refuse(node, "unhandled-syntax", "compound assignment is not modeled in the Zig source algebra subset"),
            .@"return" => self.emitReturn(node),
            .if_simple, .@"if" => self.emitIf(node),
            .while_simple, .while_cont, .@"while" => self.emitWhile(node),
            .for_simple, .@"for" => self.emitFor(node),
            .@"break" => self.emitBreakContinue(node, "zig:break"),
            .@"continue" => self.emitBreakContinue(node, "zig:continue"),
            .@"defer" => self.refuse(node, "unhandled-syntax", "defer is intentionally refused because it changes statement ordering"),
            .@"errdefer" => self.refuse(node, "unhandled-syntax", "errdefer is intentionally refused because error-union control flow is out of scope"),
            .@"switch" => self.refuse(node, "unhandled-syntax", "switch requires branch algebra not included in this draft subset"),
            else => self.emitExpr(node),
        };
    }

    fn emitLocalDecl(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const var_decl = self.fullVarDecl(node) orelse return self.refuse(node, "unhandled-syntax", "expected local var declaration");
        if (var_decl.comptime_token != null) return self.refuse(node, "unsupported-comptime", "comptime local declarations require Zig Sema");
        const name = self.varDeclName(var_decl) orelse return self.refuse(node, "unhandled-syntax", "local declaration missing name");
        const init_expr = var_decl.ast.init_node.unwrap() orelse return self.refuse(node, "unhandled-syntax", "local declarations without initializers are not lifted");
        const init_term = try self.emitExpr(init_expr);
        try appendUniqueString(self.alloc, &self.current_locals, name);
        return self.ctor("zig:decl", .{ provekit.Str(name), init_term });
    }

    fn emitAssign(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const lhs, const rhs = self.tree.nodeData(node).node_and_node;
        try self.recordWrite(lhs);
        return self.ctor("zig:assign", .{ try self.emitPlace(lhs), try self.emitExpr(rhs) });
    }

    fn emitReturn(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const maybe_value = self.tree.nodeData(node).opt_node.unwrap();
        if (maybe_value) |value| return self.ctor("zig:return", .{try self.emitExpr(value)});
        return self.ctor("zig:return", .{try self.unitTerm()});
    }

    fn emitIf(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const full = self.tree.fullIf(node) orelse return self.refuse(node, "unhandled-syntax", "expected if expression");
        if (full.payload_token != null or full.error_token != null) return self.refuse(node, "unsupported-optional", "if payload and error payload forms require optional/error-union modeling");
        const else_term = if (full.ast.else_expr.unwrap()) |else_node| try self.emitStmt(else_node) else try self.skipTerm();
        return self.ctor("zig:if", .{ try self.emitExpr(full.ast.cond_expr), try self.emitStmt(full.ast.then_expr), else_term });
    }

    fn emitWhile(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const full = self.tree.fullWhile(node) orelse return self.refuse(node, "unhandled-syntax", "expected while expression");
        if (full.inline_token != null) return self.refuse(node, "unsupported-inline-loop", "inline while requires comptime expansion");
        if (full.payload_token != null or full.error_token != null or full.ast.else_expr != .none or full.ast.cont_expr != .none) return self.refuse(node, "unsupported-loop-form", "while payloads, else branches, and continue expressions are out of scope");
        const loop_term = try self.ctor("zig:while", .{ try self.emitExpr(full.ast.cond_expr), try self.emitStmt(full.ast.then_expr) });
        try self.addOpaqueLoop(loop_term);
        return loop_term;
    }

    fn emitFor(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const full = self.tree.fullFor(node) orelse return self.refuse(node, "unhandled-syntax", "expected for expression");
        if (full.inline_token != null) return self.refuse(node, "unsupported-inline-loop", "inline for requires comptime expansion");
        if (full.ast.else_expr != .none) return self.refuse(node, "unsupported-loop-form", "for else branches are out of scope");
        if (full.ast.inputs.len != 1) return self.refuse(node, "unsupported-loop-form", "multi-input for loops are out of scope");
        const for_term = try self.ctor("zig:for", .{ try self.emitExpr(full.ast.inputs[0]), try self.emitStmt(full.ast.then_expr) });
        try self.addOpaqueLoop(for_term);
        return for_term;
    }

    fn emitBreakContinue(self: *Lifter, node: Node.Index, op_name: []const u8) anyerror!provekit.Term {
        const label, const rhs = self.tree.nodeData(node).opt_token_and_opt_node;
        if (label != .none or rhs != .none) return self.refuse(node, "unsupported-control-flow", "labeled or value-bearing break/continue is out of scope");
        return self.ctor(op_name, .{try self.unitTerm()});
    }

    fn emitExpr(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const tag = self.tree.nodeTag(node);
        return switch (tag) {
            .identifier => self.emitIdentifier(node),
            .number_literal => self.emitNumber(node),
            .char_literal => self.refuse(node, "unhandled-syntax", "character literals are not in this draft subset"),
            .string_literal, .multiline_string_literal => self.emitString(node),
            .unreachable_literal => self.emitUnreachable(node),
            .grouped_expression => self.emitGrouped(node),
            .add => self.emitBinary(node, "zig:add"),
            .sub => self.emitBinary(node, "zig:sub"),
            .mul => self.emitBinary(node, "zig:mul"),
            .div => self.emitBinary(node, "zig:div"),
            .mod => self.emitBinary(node, "zig:mod"),
            .equal_equal => self.emitBinary(node, "zig:eq"),
            .bang_equal => self.emitBinary(node, "zig:ne"),
            .less_than => self.emitBinary(node, "zig:lt"),
            .less_or_equal => self.emitBinary(node, "zig:le"),
            .greater_than => self.emitBinary(node, "zig:gt"),
            .greater_or_equal => self.emitBinary(node, "zig:ge"),
            .bool_and => self.emitBinary(node, "zig:and"),
            .bool_or => self.emitBinary(node, "zig:or"),
            .bit_and => self.emitBinary(node, "zig:bitand"),
            .bit_or => self.emitBinary(node, "zig:bitor"),
            .bit_xor => self.emitBinary(node, "zig:bitxor"),
            .shl => self.emitBinary(node, "zig:shl"),
            .shr => self.emitBinary(node, "zig:shr"),
            .negation => self.emitUnary(node, "zig:neg"),
            .bool_not => self.emitUnary(node, "zig:not"),
            .bit_not => self.emitUnary(node, "zig:bitnot"),
            .address_of => self.emitUnary(node, "zig:addr"),
            .deref => self.emitUnary(node, "zig:deref"),
            .field_access => self.emitField(node),
            .array_access => self.emitIndex(node),
            .call, .call_comma, .call_one, .call_one_comma => self.emitCall(node),
            .builtin_call, .builtin_call_comma, .builtin_call_two, .builtin_call_two_comma => self.emitBuiltinCall(node),
            .assign => self.emitAssign(node),
            .if_simple, .@"if" => self.emitIf(node),
            .block, .block_semicolon, .block_two, .block_two_semicolon => self.emitBlock(node),
            .@"return" => self.emitReturn(node),
            .@"switch" => self.refuse(node, "unhandled-syntax", "switch requires branch algebra not included in this draft subset"),
            .@"try", .@"catch", .@"orelse", .error_union, .error_set_decl, .error_value => self.refuse(node, "unsupported-error-union", "error unions, try, catch, and orelse require error-flow modeling"),
            .@"comptime" => self.refuse(node, "unsupported-comptime", "comptime expressions require Zig Sema"),
            .@"suspend", .@"resume" => self.refuse(node, "unsupported-async", "suspend/resume are out of scope"),
            .struct_init, .struct_init_comma, .struct_init_one, .struct_init_one_comma, .struct_init_dot, .struct_init_dot_comma, .struct_init_dot_two, .struct_init_dot_two_comma => self.refuse(node, "unhandled-syntax", "struct initialization is not modeled in this draft subset"),
            .array_init, .array_init_comma, .array_init_one, .array_init_one_comma, .array_init_dot, .array_init_dot_comma, .array_init_dot_two, .array_init_dot_two_comma => self.refuse(node, "unhandled-syntax", "array initialization is not modeled in this draft subset"),
            else => self.refuseFmt(node, "unhandled-syntax", "unhandled Zig AST node tag: {s}", .{@tagName(tag)}),
        };
    }

    fn emitIdentifier(self: *Lifter, node: Node.Index) !provekit.Term {
        const name = self.tree.tokenSlice(self.tree.nodeMainToken(node));
        if (self.isGlobal(name)) try self.addEffect(.{ .reads = name });
        if (std.mem.eql(u8, name, "true")) return provekit.BoolConst(true);
        if (std.mem.eql(u8, name, "false")) return provekit.BoolConst(false);
        return provekit.Var(name);
    }

    fn emitNumber(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const text = self.tree.tokenSlice(self.tree.nodeMainToken(node));
        const value = std.fmt.parseInt(i64, text, 0) catch return self.refuse(node, "unsupported-literal", "only integer literals that fit i64 are lifted");
        return provekit.Num(value);
    }

    fn emitString(self: *Lifter, node: Node.Index) !provekit.Term {
        switch (self.tree.nodeTag(node)) {
            .string_literal => {
                const raw = self.tree.getNodeSource(node);
                // raw includes the surrounding double-quotes; parseAlloc decodes all Zig escapes.
                const decoded = std.zig.string_literal.parseAlloc(self.alloc, raw) catch |err| switch (err) {
                    error.InvalidLiteral => return self.refuse(node, "invalid-string-literal", "string literal contains an invalid escape sequence"),
                    error.OutOfMemory => return error.OutOfMemory,
                };
                return provekit.Str(decoded);
            },
            .multiline_string_literal => {
                // A multiline string is a sequence of `.multiline_string_literal_line` tokens.
                // Each token's slice includes the `\\` prefix and a trailing newline.
                // Decoded value: strip `\\ ` (or just `\\` when line is empty), join lines.
                // No escape sequences exist inside multiline strings; they are raw bytes.
                const first_tok = self.tree.nodeData(node).token_and_token[0];
                const last_tok = self.tree.nodeData(node).token_and_token[1];
                var out = std.ArrayList(u8).empty;
                var tok = first_tok;
                while (tok <= last_tok) : (tok += 1) {
                    const line = self.tree.tokenSlice(tok);
                    // Each line token starts with `\\` (two bytes). Strip them.
                    // The token may or may not include a trailing newline depending on whether
                    // it's the last line. To be safe, strip a leading `\\` then any space char,
                    // then strip a trailing `\n` only between lines (not after the last).
                    const content = if (line.len >= 2) line[2..] else line;
                    // Strip trailing newline that the tokenizer includes.
                    const trimmed = if (content.len > 0 and content[content.len - 1] == '\n')
                        content[0 .. content.len - 1]
                    else
                        content;
                    if (tok > first_tok) try out.append(self.alloc, '\n');
                    try out.appendSlice(self.alloc, trimmed);
                }
                return provekit.Str(try out.toOwnedSlice(self.alloc));
            },
            else => unreachable,
        }
    }

    fn emitUnreachable(self: *Lifter, node: Node.Index) !provekit.Term {
        _ = node;
        try self.addEffect(.panics);
        return self.ctor("zig:unreachable", .{});
    }

    fn emitGrouped(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const child = self.tree.nodeData(node).node_and_token[0];
        return self.emitExpr(child);
    }

    fn emitBinary(self: *Lifter, node: Node.Index, op_name: []const u8) anyerror!provekit.Term {
        const lhs, const rhs = self.tree.nodeData(node).node_and_node;
        return self.ctor(op_name, .{ try self.emitExpr(lhs), try self.emitExpr(rhs) });
    }

    fn emitUnary(self: *Lifter, node: Node.Index, op_name: []const u8) anyerror!provekit.Term {
        return self.ctor(op_name, .{try self.emitExpr(self.tree.nodeData(node).node)});
    }

    fn emitField(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const base, const field_token = self.tree.nodeData(node).node_and_token;
        return self.ctor("zig:field", .{ try self.emitExpr(base), provekit.Str(self.tree.tokenSlice(field_token)) });
    }

    fn emitIndex(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const lhs, const rhs = self.tree.nodeData(node).node_and_node;
        return self.ctor("zig:index", .{ try self.emitExpr(lhs), try self.emitExpr(rhs) });
    }

    fn emitPlace(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        return switch (self.tree.nodeTag(node)) {
            .identifier, .field_access, .array_access, .deref => self.emitExpr(node),
            else => self.refuse(node, "unsupported-assignment-target", "assignment target is not a modeled lvalue"),
        };
    }

    fn emitCall(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        var buffer: [1]Node.Index = undefined;
        const call = self.tree.fullCall(&buffer, node) orelse return self.refuse(node, "unhandled-syntax", "expected call expression");
        const callee_name = try self.exprName(call.ast.fn_expr) orelse return self.refuse(node, "unsupported-call", "callee expression is not a stable name");
        if (isIoCallee(callee_name)) {
            try self.addEffect(.io);
        } else if (!self.isKnownFunction(callee_name)) {
            try self.addEffect(.{ .unresolved_call = callee_name });
        }
        var args = std.ArrayList(provekit.Term).empty;
        try args.append(self.alloc, provekit.Str(callee_name));
        for (call.ast.params) |param| try args.append(self.alloc, try self.emitExpr(param));
        return provekit.Ctor("zig:call", try args.toOwnedSlice(self.alloc));
    }

    fn emitBuiltinCall(self: *Lifter, node: Node.Index) anyerror!provekit.Term {
        const name = self.tree.tokenSlice(self.tree.nodeMainToken(node));
        var buffer: [2]Node.Index = undefined;
        const params = self.tree.builtinCallParams(&buffer, node) orelse &.{};
        if (std.mem.eql(u8, name, "@panic")) {
            try self.addEffect(.panics);
            var args = std.ArrayList(provekit.Term).empty;
            for (params) |param| try args.append(self.alloc, try self.emitExpr(param));
            return provekit.Ctor("zig:panic", try args.toOwnedSlice(self.alloc));
        }
        if (std.mem.eql(u8, name, "@as")) {
            if (params.len != 2) return self.refuse(node, "unsupported-builtin", "@as must have exactly two arguments");
            return self.ctor("zig:cast", .{ provekit.Str(self.tree.getNodeSource(params[0])), try self.emitExpr(params[1]) });
        }
        return self.refuseFmt(node, "unsupported-builtin", "builtin {s} is not modeled by the Zig source lifter", .{name});
    }

    fn recordWrite(self: *Lifter, lhs: Node.Index) anyerror!void {
        switch (self.tree.nodeTag(lhs)) {
            .identifier => {
                const name = self.tree.tokenSlice(self.tree.nodeMainToken(lhs));
                if (!self.isLocal(name)) try self.addEffect(.{ .writes = name });
            },
            .deref => try self.addEffect(.{ .writes = try self.nodeSourceOwned(lhs) }),
            .field_access => {
                if (try self.fieldBaseIsNonLocal(lhs)) try self.addEffect(.{ .writes = try self.nodeSourceOwned(lhs) });
            },
            .array_access => {
                const lhs_node = self.tree.nodeData(lhs).node_and_node[0];
                if (try self.baseIsNonLocal(lhs_node)) try self.addEffect(.{ .writes = try self.nodeSourceOwned(lhs) });
            },
            else => return self.refuse(lhs, "unsupported-assignment-target", "assignment target is not a modeled lvalue"),
        }
    }

    fn fieldBaseIsNonLocal(self: *Lifter, node: Node.Index) anyerror!bool {
        const base = self.tree.nodeData(node).node_and_token[0];
        return self.baseIsNonLocal(base);
    }

    fn baseIsNonLocal(self: *Lifter, node: Node.Index) anyerror!bool {
        return switch (self.tree.nodeTag(node)) {
            .identifier => blk: {
                const name = self.tree.tokenSlice(self.tree.nodeMainToken(node));
                break :blk !self.isLocal(name);
            },
            .field_access => self.fieldBaseIsNonLocal(node),
            .deref => true,
            else => false,
        };
    }

    fn addOpaqueLoop(self: *Lifter, loop_term: provekit.Term) !void {
        const bytes = try provekit.jcsStringify(self.alloc, loop_term);
        const cid = try provekit.jcsHash(self.alloc, bytes);
        try self.addEffect(.{ .opaque_loop = cid });
    }

    fn sourceUnitContract(self: *Lifter) !FunctionContract {
        var body: ?provekit.Term = null;
        for (self.declarations.items) |decl| {
            body = if (body) |prev| try self.ctor("zig:seq", .{ prev, decl.bodyTerm() }) else decl.bodyTerm();
        }
        const operational = body orelse try self.skipTerm();
        const source_unit_term = try self.ctor("zig:source-unit", .{ provekit.Str(self.source), operational });
        const post_terms = try termArgs(self.alloc, .{ provekit.Var("return_value"), source_unit_term });
        return .{
            .fn_name = try std.fmt.allocPrint(self.alloc, "<source-unit:{s}>", .{self.path}),
            .formals = &.{},
            .formal_sorts = &.{},
            .return_sort = .{ .primitive = "Stmt" },
            .pre = trueFormula(),
            .post = provekit.Atomic("=", post_terms),
            .body_cid = null,
            .effects = &.{},
            .locus = .{ .file = self.path, .line = 1, .col = 1 },
            .body_term = source_unit_term,
        };
    }

    fn sortFromType(self: *Lifter, type_node: Node.Index, is_return: bool) anyerror!provekit.Sort {
        const text = std.mem.trim(u8, self.tree.getNodeSource(type_node), " \t\r\n");
        if (std.mem.eql(u8, text, "void")) return .{ .primitive = "Unit" };
        if (std.mem.eql(u8, text, "bool")) return provekit.Sort.Bool;
        if (std.mem.eql(u8, text, "usize") or std.mem.eql(u8, text, "isize") or std.mem.eql(u8, text, "comptime_int") or isIntegerType(text)) return provekit.Sort.Int;
        if (std.mem.eql(u8, text, "[]const u8") or std.mem.eql(u8, text, "[]u8")) return provekit.Sort.String;
        if (std.mem.startsWith(u8, text, "*") or std.mem.startsWith(u8, text, "[]")) return .{ .primitive = "Ptr" };
        if (self.tree.nodeTag(type_node) == .error_union) return self.refuse(type_node, "unsupported-error-union", "error union types require error-flow modeling");
        if (self.tree.nodeTag(type_node) == .optional_type) return self.refuse(type_node, "unsupported-optional", "optional types require optional-flow modeling");
        if (is_return) return self.refuseFmt(type_node, "unsupported-return-sort", "unsupported return type: {s}", .{text});
        return self.refuseFmt(type_node, "unsupported-formal-sort", "unsupported parameter type: {s}", .{text});
    }

    fn fullVarDecl(self: *Lifter, node: Node.Index) ?Ast.full.VarDecl {
        return switch (self.tree.nodeTag(node)) {
            .global_var_decl => self.tree.globalVarDecl(node),
            .local_var_decl => self.tree.localVarDecl(node),
            .simple_var_decl => self.tree.simpleVarDecl(node),
            .aligned_var_decl => self.tree.alignedVarDecl(node),
            else => null,
        };
    }

    fn varDeclName(self: *Lifter, var_decl: Ast.full.VarDecl) ?[]const u8 {
        const name_token = var_decl.ast.mut_token + 1;
        if (self.tree.tokenTag(name_token) != .identifier) return null;
        return self.tree.tokenSlice(name_token);
    }

    fn isContainerDecl(self: *Lifter, node: Node.Index) bool {
        return switch (self.tree.nodeTag(node)) {
            .container_decl, .container_decl_trailing, .container_decl_two, .container_decl_two_trailing, .container_decl_arg, .container_decl_arg_trailing => true,
            else => false,
        };
    }

    fn containerMembers(self: *Lifter, node: Node.Index) ?[]const Node.Index {
        var buffer: [2]Node.Index = undefined;
        const decl = self.tree.fullContainerDecl(&buffer, node) orelse return null;
        return decl.ast.members;
    }

    fn qualifiedName(self: *Lifter, namespace: []const []const u8, name: []const u8) ![]const u8 {
        var out = std.ArrayList(u8).empty;
        try out.appendSlice(self.alloc, self.path);
        for (namespace) |part| {
            try out.append(self.alloc, '.');
            try out.appendSlice(self.alloc, part);
        }
        try out.append(self.alloc, '.');
        try out.appendSlice(self.alloc, name);
        return try out.toOwnedSlice(self.alloc);
    }

    fn exprName(self: *Lifter, node: Node.Index) anyerror!?[]const u8 {
        return switch (self.tree.nodeTag(node)) {
            .identifier => self.tree.tokenSlice(self.tree.nodeMainToken(node)),
            .field_access => blk: {
                const base, const field_token = self.tree.nodeData(node).node_and_token;
                const base_name = try self.exprName(base) orelse break :blk null;
                break :blk try std.fmt.allocPrint(self.alloc, "{s}.{s}", .{ base_name, self.tree.tokenSlice(field_token) });
            },
            .builtin_call, .builtin_call_comma, .builtin_call_two, .builtin_call_two_comma => self.tree.tokenSlice(self.tree.nodeMainToken(node)),
            else => null,
        };
    }

    fn isLocal(self: *Lifter, name: []const u8) bool {
        return containsString(self.current_locals.items, name);
    }

    fn isGlobal(self: *Lifter, name: []const u8) bool {
        for (self.globals.items) |global| if (std.mem.eql(u8, global.name, name)) return true;
        return false;
    }

    fn isKnownFunction(self: *Lifter, name: []const u8) bool {
        return containsString(self.known_functions.items, name);
    }

    fn addEffect(self: *Lifter, effect: Effect) !void {
        for (self.current_effects.items) |existing| if (Effect.eql(existing, effect)) return;
        try self.current_effects.append(self.alloc, effect);
    }

    fn ctor(self: *Lifter, name: []const u8, items: anytype) !provekit.Term {
        return provekit.Ctor(name, try termArgs(self.alloc, items));
    }

    fn unitTerm(self: *Lifter) !provekit.Term {
        return self.ctor("zig:unit", .{});
    }

    fn skipTerm(self: *Lifter) !provekit.Term {
        return self.ctor("zig:skip", .{try self.unitTerm()});
    }

    fn nodeSourceOwned(self: *Lifter, node: Node.Index) ![]const u8 {
        return try self.alloc.dupe(u8, std.mem.trim(u8, self.tree.getNodeSource(node), " \t\r\n"));
    }

    fn locusOf(self: *Lifter, node: Node.Index) Locus {
        const loc = self.tree.tokenLocation(0, self.tree.nodeMainToken(node));
        return .{ .file = self.path, .line = loc.line + 1, .col = loc.column + 1 };
    }

    fn refuse(self: *Lifter, node: Node.Index, kind: []const u8, reason: []const u8) LiftError {
        self.refusals.append(self.alloc, .{
            .kind = kind,
            .function = self.current_function,
            .line = self.locusOf(node).line,
            .reason = reason,
        }) catch {};
        return error.Refused;
    }

    fn refuseFmt(self: *Lifter, node: Node.Index, kind: []const u8, comptime fmt: []const u8, args: anytype) LiftError {
        const reason = std.fmt.allocPrint(self.alloc, fmt, args) catch fmt;
        return self.refuse(node, kind, reason);
    }
};

pub fn liftSource(alloc: std.mem.Allocator, source: []const u8, path: []const u8) !LiftOutput {
    const path_owned = try alloc.dupe(u8, path);
    const source_z = try alloc.allocSentinel(u8, source.len, 0);
    @memcpy(source_z[0..source.len], source);
    var tree = try Ast.parse(alloc, source_z, .zig);
    defer tree.deinit(alloc);

    var lifter = Lifter.init(alloc, &tree, source_z[0..source.len], path_owned);
    if (tree.errors.len > 0) {
        for (tree.errors) |parse_error| {
            const loc = tree.tokenLocation(0, parse_error.token);
            const reason = try std.fmt.allocPrint(alloc, "parse error: {s}", .{@tagName(parse_error.tag)});
            try lifter.refusals.append(alloc, .{ .kind = "parse-error", .function = null, .line = loc.line + 1, .reason = reason });
        }
        return .{ .declarations = &.{}, .refusals = try lifter.refusals.toOwnedSlice(alloc) };
    }
    return lifter.lift();
}

pub fn sugarBodySourceForFunc(
    alloc: std.mem.Allocator,
    source: []const u8,
    path: []const u8,
    fn_name: []const u8,
) !?SugarBodySource {
    const source_z = try alloc.allocSentinel(u8, source.len, 0);
    @memcpy(source_z[0..source.len], source);
    var tree = try Ast.parse(alloc, source_z, .zig);
    defer tree.deinit(alloc);
    if (tree.errors.len > 0) return null;

    const fn_node = findFunctionByName(&tree, fn_name) orelse return null;
    return try sugarBodySourceForNode(alloc, &tree, path, fn_node);
}

pub fn recognizeSourcePaths(
    alloc: std.mem.Allocator,
    io: std.Io,
    project_root: []const u8,
    source_paths: []const []const u8,
    bindings: []const BindingTemplate,
) !RecognizeResponse {
    var tags: std.ArrayList(RecognizeTag) = .empty;
    var root = try std.Io.Dir.openDir(std.Io.Dir.cwd(), io, project_root, .{});
    defer root.close(io);
    for (source_paths) |rel_path| {
        const source = std.Io.Dir.readFileAlloc(root, io, rel_path, alloc, .unlimited) catch continue;
        try recognizeSource(alloc, rel_path, source, bindings, &tags);
    }
    return .{ .tags = try tags.toOwnedSlice(alloc) };
}

pub fn recognizeSourceText(
    alloc: std.mem.Allocator,
    rel_path: []const u8,
    source: []const u8,
    bindings: []const BindingTemplate,
) !RecognizeResponse {
    var tags: std.ArrayList(RecognizeTag) = .empty;
    try recognizeSource(alloc, rel_path, source, bindings, &tags);
    return .{ .tags = try tags.toOwnedSlice(alloc) };
}

fn recognizeSource(
    alloc: std.mem.Allocator,
    rel_path: []const u8,
    source: []const u8,
    bindings: []const BindingTemplate,
    tags: *std.ArrayList(RecognizeTag),
) !void {
    const source_z = try alloc.allocSentinel(u8, source.len, 0);
    @memcpy(source_z[0..source.len], source);
    var tree = try Ast.parse(alloc, source_z, .zig);
    defer tree.deinit(alloc);
    if (tree.errors.len > 0) return;

    var functions: std.ArrayList(Node.Index) = .empty;
    const roots = tree.rootDecls();
    for (roots) |decl| try collectFunctionNodes(alloc, &tree, decl, &functions);
    for (functions.items) |fn_node| {
        const candidate = try sugarBodySourceForNode(alloc, &tree, rel_path, fn_node);
        const binding = bindingByTemplateCid(bindings, candidate.template_cid) orelse continue;
        const function_name = functionName(&tree, fn_node) orelse continue;
        try tags.append(alloc, .{
            .file = rel_path,
            .span = candidate.span,
            .function_name = try alloc.dupe(u8, function_name),
            .concept_name = binding.concept_name,
            .library_tag = binding.library_tag orelse binding.target_library_tag,
            .family = binding.family,
            .template_cid = candidate.template_cid,
            .contract_cid = binding.contract_cid,
            .match_tier = "exact",
            .param_bindings = try paramBindings(alloc, candidate.param_names),
        });
    }
}

fn bindingByTemplateCid(bindings: []const BindingTemplate, template_cid: []const u8) ?BindingTemplate {
    for (bindings) |binding| {
        if (std.mem.eql(u8, binding.template_cid, template_cid)) return binding;
    }
    return null;
}

fn paramBindings(alloc: std.mem.Allocator, names: []const []const u8) ![]const ParamBinding {
    var out: std.ArrayList(ParamBinding) = .empty;
    for (names, 0..) |name, i| {
        try out.append(alloc, .{ .index = i + 1, .source_text = name });
    }
    return out.toOwnedSlice(alloc);
}

fn sugarBodySourceForNode(
    alloc: std.mem.Allocator,
    tree: *const Ast,
    path: []const u8,
    fn_node: Node.Index,
) !SugarBodySource {
    const body_node = functionBodyNode(tree, fn_node).?;
    const body_text = try alloc.dupe(u8, blockInnerSource(tree, body_node));
    const param_names = try functionParamNames(alloc, tree, fn_node);
    var builder = TemplateBuilder.init(alloc, tree, param_names);
    const ast_template = try builder.block(body_node);
    const template_bytes = try provekit.jcsStringify(alloc, ast_template);
    return .{
        .file = path,
        .span = functionSpan(tree, fn_node),
        .source_cid = try provekit.jcsHash(alloc, body_text),
        .body_text = body_text,
        .ast_template = ast_template,
        .template_cid = try provekit.jcsHash(alloc, template_bytes),
        .param_names = param_names,
    };
}

const TemplateBuilder = struct {
    alloc: std.mem.Allocator,
    tree: *const Ast,
    params: []const []const u8,

    fn init(alloc: std.mem.Allocator, tree: *const Ast, params: []const []const u8) TemplateBuilder {
        return .{ .alloc = alloc, .tree = tree, .params = params };
    }

    fn box(self: *TemplateBuilder, value: SugarAstTemplate) !*const SugarAstTemplate {
        const ptr = try self.alloc.create(SugarAstTemplate);
        ptr.* = value;
        return ptr;
    }

    fn block(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        var buf: [2]Node.Index = undefined;
        const stmts = self.tree.blockStatements(&buf, node) orelse return .{ .other = @tagName(self.tree.nodeTag(node)) };
        var out: std.ArrayList(SugarAstTemplate) = .empty;
        for (stmts) |stmt_node| try out.append(self.alloc, try self.stmt(stmt_node));
        return .{ .block = .{ .stmts = try out.toOwnedSlice(self.alloc) } };
    }

    fn stmt(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        return switch (self.tree.nodeTag(node)) {
            .block, .block_semicolon, .block_two, .block_two_semicolon => self.block(node),
            .local_var_decl, .simple_var_decl, .aligned_var_decl => self.letStmt(node),
            .@"return" => .{ .expr_stmt = .{ .expr = try self.box(try self.returnExpr(node)), .trailing_semi = false } },
            else => .{ .expr_stmt = .{ .expr = try self.box(try self.expr(node)), .trailing_semi = true } },
        };
    }

    fn letStmt(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        const var_decl = fullVarDeclFor(self.tree, node) orelse return .{ .other = @tagName(self.tree.nodeTag(node)) };
        const name = varDeclNameFor(self.tree, var_decl) orelse "_";
        const initializer = if (var_decl.ast.init_node.unwrap()) |init_node| try self.box(try self.expr(init_node)) else null;
        return .{ .let = .{ .pat = try self.box(.{ .binding = name }), .init = initializer } };
    }

    fn returnExpr(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        const maybe_value = self.tree.nodeData(node).opt_node.unwrap();
        const expr_template = if (maybe_value) |value| try self.box(try self.expr(value)) else null;
        return .{ .@"return" = expr_template };
    }

    fn expr(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        const tag = self.tree.nodeTag(node);
        return switch (tag) {
            .identifier => self.ident(node),
            .number_literal => .{ .lit = .{ .ty = "number", .value = self.tree.tokenSlice(self.tree.nodeMainToken(node)) } },
            .string_literal, .multiline_string_literal => .{ .lit = .{ .ty = "string", .value = self.tree.getNodeSource(node) } },
            .grouped_expression => self.expr(self.tree.nodeData(node).node_and_token[0]),
            .add => self.binary(node, "+"),
            .sub => self.binary(node, "-"),
            .mul => self.binary(node, "*"),
            .div => self.binary(node, "/"),
            .mod => self.binary(node, "%"),
            .equal_equal => self.binary(node, "=="),
            .bang_equal => self.binary(node, "!="),
            .less_than => self.binary(node, "<"),
            .less_or_equal => self.binary(node, "<="),
            .greater_than => self.binary(node, ">"),
            .greater_or_equal => self.binary(node, ">="),
            .bool_and => self.binary(node, "and"),
            .bool_or => self.binary(node, "or"),
            .bit_and => self.binary(node, "&"),
            .bit_or => self.binary(node, "|"),
            .bit_xor => self.binary(node, "^"),
            .call, .call_comma, .call_one, .call_one_comma => self.call(node),
            .field_access => self.fieldAccess(node),
            .address_of => .{ .ref = .{ .mutability = false, .expr = try self.box(try self.expr(self.tree.nodeData(node).node)) } },
            .@"return" => self.returnExpr(node),
            .block, .block_semicolon, .block_two, .block_two_semicolon => self.block(node),
            else => .{ .other = @tagName(tag) },
        };
    }

    fn ident(self: *TemplateBuilder, node: Node.Index) SugarAstTemplate {
        const name = self.tree.tokenSlice(self.tree.nodeMainToken(node));
        for (self.params, 0..) |param, i| {
            if (std.mem.eql(u8, param, name)) return .{ .param_ref = i + 1 };
        }
        return .{ .ident = name };
    }

    fn binary(self: *TemplateBuilder, node: Node.Index, op: []const u8) anyerror!SugarAstTemplate {
        const lhs, const rhs = self.tree.nodeData(node).node_and_node;
        return .{ .binary = .{
            .op = op,
            .left = try self.box(try self.expr(lhs)),
            .right = try self.box(try self.expr(rhs)),
        } };
    }

    fn call(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        var buffer: [1]Node.Index = undefined;
        const call_data = self.tree.fullCall(&buffer, node) orelse return .{ .other = @tagName(self.tree.nodeTag(node)) };
        var args: std.ArrayList(SugarAstTemplate) = .empty;
        for (call_data.ast.params) |param| try args.append(self.alloc, try self.expr(param));
        if (self.tree.nodeTag(call_data.ast.fn_expr) == .field_access) {
            const base, const field_token = self.tree.nodeData(call_data.ast.fn_expr).node_and_token;
            return .{ .method_call = .{
                .receiver = try self.box(try self.expr(base)),
                .method = self.tree.tokenSlice(field_token),
                .args = try args.toOwnedSlice(self.alloc),
            } };
        }
        return .{ .call = .{
            .func = try self.box(try self.expr(call_data.ast.fn_expr)),
            .args = try args.toOwnedSlice(self.alloc),
        } };
    }

    fn fieldAccess(self: *TemplateBuilder, node: Node.Index) anyerror!SugarAstTemplate {
        const base, const field_token = self.tree.nodeData(node).node_and_token;
        return .{ .path = try self.pathSegments(base, self.tree.tokenSlice(field_token)) };
    }

    fn pathSegments(self: *TemplateBuilder, base: Node.Index, field: []const u8) ![]const []const u8 {
        var out: std.ArrayList([]const u8) = .empty;
        try self.collectPathSegments(base, &out);
        try out.append(self.alloc, field);
        return try out.toOwnedSlice(self.alloc);
    }

    fn collectPathSegments(self: *TemplateBuilder, node: Node.Index, out: *std.ArrayList([]const u8)) anyerror!void {
        switch (self.tree.nodeTag(node)) {
            .identifier => try out.append(self.alloc, self.tree.tokenSlice(self.tree.nodeMainToken(node))),
            .field_access => {
                const base, const field_token = self.tree.nodeData(node).node_and_token;
                try self.collectPathSegments(base, out);
                try out.append(self.alloc, self.tree.tokenSlice(field_token));
            },
            else => {},
        }
    }
};

fn findFunctionByName(tree: *const Ast, fn_name: []const u8) ?Node.Index {
    const roots = tree.rootDecls();
    for (roots) |decl| {
        if (findFunctionByNameInNode(tree, decl, fn_name)) |node| return node;
    }
    return null;
}

fn findFunctionByNameInNode(tree: *const Ast, node: Node.Index, fn_name: []const u8) ?Node.Index {
    switch (tree.nodeTag(node)) {
        .fn_decl => {
            const name = functionName(tree, node) orelse return null;
            if (std.mem.eql(u8, name, fn_name)) return node;
        },
        .global_var_decl, .simple_var_decl, .aligned_var_decl => {
            const var_decl = fullVarDeclFor(tree, node) orelse return null;
            const init_node = var_decl.ast.init_node.unwrap() orelse return null;
            if (!isContainerDeclFor(tree, init_node)) return null;
            const members = containerMembersFor(tree, init_node) orelse return null;
            for (members) |member| {
                if (findFunctionByNameInNode(tree, member, fn_name)) |found| return found;
            }
        },
        else => {},
    }
    return null;
}

fn collectFunctionNodes(
    alloc: std.mem.Allocator,
    tree: *const Ast,
    node: Node.Index,
    functions: *std.ArrayList(Node.Index),
) !void {
    switch (tree.nodeTag(node)) {
        .fn_decl => try functions.append(alloc, node),
        .global_var_decl, .simple_var_decl, .aligned_var_decl => {
            const var_decl = fullVarDeclFor(tree, node) orelse return;
            const init_node = var_decl.ast.init_node.unwrap() orelse return;
            if (!isContainerDeclFor(tree, init_node)) return;
            const members = containerMembersFor(tree, init_node) orelse return;
            for (members) |member| try collectFunctionNodes(alloc, tree, member, functions);
        },
        else => {},
    }
}

fn functionName(tree: *const Ast, fn_node: Node.Index) ?[]const u8 {
    var buffer: [1]Node.Index = undefined;
    const proto_node = tree.nodeData(fn_node).node_and_node[0];
    const proto = tree.fullFnProto(&buffer, proto_node) orelse return null;
    const name_token = proto.name_token orelse return null;
    return tree.tokenSlice(name_token);
}

fn functionBodyNode(tree: *const Ast, fn_node: Node.Index) ?Node.Index {
    if (tree.nodeTag(fn_node) != .fn_decl) return null;
    return tree.nodeData(fn_node).node_and_node[1];
}

fn functionParamNames(alloc: std.mem.Allocator, tree: *const Ast, fn_node: Node.Index) ![]const []const u8 {
    var buffer: [1]Node.Index = undefined;
    const proto_node = tree.nodeData(fn_node).node_and_node[0];
    const proto = tree.fullFnProto(&buffer, proto_node) orelse return &.{};
    var names: std.ArrayList([]const u8) = .empty;
    var it = proto.iterate(tree);
    while (it.next()) |param| {
        if (param.name_token) |tok| try names.append(alloc, tree.tokenSlice(tok));
    }
    return names.toOwnedSlice(alloc);
}

fn functionSpan(tree: *const Ast, fn_node: Node.Index) SourceSpan {
    const start = tree.tokenLocation(0, tree.nodeMainToken(fn_node));
    const body_node = functionBodyNode(tree, fn_node) orelse return .{
        .start_line = start.line + 1,
        .start_col = start.column,
        .end_line = start.line + 1,
        .end_col = start.column,
    };
    const block_src = tree.getNodeSource(body_node);
    var line = start.line + 1;
    var col = start.column;
    for (block_src) |c| {
        if (c == '\n') {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    return .{
        .start_line = start.line + 1,
        .start_col = start.column,
        .end_line = line,
        .end_col = col,
    };
}

fn blockInnerSource(tree: *const Ast, body_node: Node.Index) []const u8 {
    const block_src = tree.getNodeSource(body_node);
    const open = std.mem.indexOfScalar(u8, block_src, '{') orelse return "";
    const close = std.mem.lastIndexOfScalar(u8, block_src, '}') orelse return "";
    if (close <= open) return "";
    return std.mem.trim(u8, block_src[open + 1 .. close], " \t\r\n");
}

fn fullVarDeclFor(tree: *const Ast, node: Node.Index) ?Ast.full.VarDecl {
    return switch (tree.nodeTag(node)) {
        .global_var_decl => tree.globalVarDecl(node),
        .local_var_decl => tree.localVarDecl(node),
        .simple_var_decl => tree.simpleVarDecl(node),
        .aligned_var_decl => tree.alignedVarDecl(node),
        else => null,
    };
}

fn varDeclNameFor(tree: *const Ast, var_decl: Ast.full.VarDecl) ?[]const u8 {
    const name_token = var_decl.ast.mut_token + 1;
    if (tree.tokenTag(name_token) != .identifier) return null;
    return tree.tokenSlice(name_token);
}

fn isContainerDeclFor(tree: *const Ast, node: Node.Index) bool {
    return switch (tree.nodeTag(node)) {
        .container_decl, .container_decl_trailing, .container_decl_two, .container_decl_two_trailing, .container_decl_arg, .container_decl_arg_trailing => true,
        else => false,
    };
}

fn containerMembersFor(tree: *const Ast, node: Node.Index) ?[]const Node.Index {
    var buffer: [2]Node.Index = undefined;
    const decl = tree.fullContainerDecl(&buffer, node) orelse return null;
    return decl.ast.members;
}

pub fn verifyContracts(alloc: std.mem.Allocator, declarations: []const FunctionContract) ![]FunctionContract {
    var out: std.ArrayList(FunctionContract) = .empty;
    for (declarations) |decl| {
        if (std.mem.startsWith(u8, decl.fn_name, "<source-unit:")) continue;
        try out.append(alloc, try verifyContract(alloc, decl));
    }
    return out.toOwnedSlice(alloc);
}

fn verifyContract(alloc: std.mem.Allocator, decl: FunctionContract) !FunctionContract {
    var out = decl;
    out.bridge_source_symbol = try bareSymbol(alloc, decl.fn_name);
    out.pre = try normalizeFormulaForVerify(alloc, decl.pre);
    out.post = try normalizePostForVerify(alloc, decl.post);
    out.body_term = try normalizeTermForVerify(alloc, decl.body_term);
    return out;
}

fn normalizePostForVerify(alloc: std.mem.Allocator, formula: provekit.Formula) !provekit.Formula {
    return switch (formula) {
        .atomic => |a| blk: {
            if (std.mem.eql(u8, a.name, "=") and a.args.len == 2) {
                const args = try termArgs(alloc, .{
                    provekit.Var("result"),
                    try normalizeTermForVerify(alloc, unwrapReturn(a.args[1])),
                });
                break :blk provekit.Atomic("=", args);
            }
            break :blk try normalizeFormulaForVerify(alloc, formula);
        },
        else => try normalizeFormulaForVerify(alloc, formula),
    };
}

fn normalizeFormulaForVerify(alloc: std.mem.Allocator, formula: provekit.Formula) anyerror!provekit.Formula {
    return switch (formula) {
        .atomic => |a| provekit.Atomic(normalizeAtomicName(a.name), try normalizeTermSliceForVerify(alloc, a.args)),
        .connective => |c| .{ .connective = .{
            .kind = c.kind,
            .operands = try normalizeFormulaSliceForVerify(alloc, c.operands),
        } },
        .quantifier => |q| blk: {
            const body = try alloc.create(provekit.Formula);
            body.* = try normalizeFormulaForVerify(alloc, q.body.*);
            break :blk .{ .quantifier = .{
                .kind = q.kind,
                .name = q.name,
                .sort = q.sort,
                .body = body,
            } };
        },
    };
}

fn normalizeFormulaSliceForVerify(alloc: std.mem.Allocator, formulas: []const provekit.Formula) ![]const provekit.Formula {
    var out = try alloc.alloc(provekit.Formula, formulas.len);
    for (formulas, 0..) |formula, i| out[i] = try normalizeFormulaForVerify(alloc, formula);
    return out;
}

fn normalizeTermSliceForVerify(alloc: std.mem.Allocator, terms: []const provekit.Term) ![]const provekit.Term {
    var out = try alloc.alloc(provekit.Term, terms.len);
    for (terms, 0..) |term, i| out[i] = try normalizeTermForVerify(alloc, term);
    return out;
}

fn normalizeTermForVerify(alloc: std.mem.Allocator, term: provekit.Term) anyerror!provekit.Term {
    return switch (term) {
        .var_term => |v| if (std.mem.eql(u8, v.name, "return_value")) provekit.Var("result") else term,
        .const_term => term,
        .ctor_term => |c| blk: {
            if (std.mem.eql(u8, c.name, "zig:return")) {
                break :blk try normalizeTermForVerify(alloc, unwrapReturn(term));
            }
            break :blk provekit.Ctor(normalizeCtorName(c.name), try normalizeTermSliceForVerify(alloc, c.args));
        },
    };
}

fn unwrapReturn(term: provekit.Term) provekit.Term {
    return switch (term) {
        .ctor_term => |c| if (std.mem.eql(u8, c.name, "zig:return") and c.args.len == 1) c.args[0] else term,
        else => term,
    };
}

fn normalizeCtorName(name: []const u8) []const u8 {
    if (std.mem.eql(u8, name, "zig:add")) return "+";
    if (std.mem.eql(u8, name, "zig:sub")) return "-";
    if (std.mem.eql(u8, name, "zig:mul")) return "*";
    if (std.mem.eql(u8, name, "zig:div")) return "/";
    if (std.mem.eql(u8, name, "zig:mod")) return "mod";
    return name;
}

fn normalizeAtomicName(name: []const u8) []const u8 {
    if (std.mem.eql(u8, name, "zig:eq")) return "=";
    if (std.mem.eql(u8, name, "zig:ne")) return "!=";
    if (std.mem.eql(u8, name, "zig:lt")) return "<";
    if (std.mem.eql(u8, name, "zig:le")) return "<=";
    if (std.mem.eql(u8, name, "zig:gt")) return ">";
    if (std.mem.eql(u8, name, "zig:ge")) return ">=";
    return name;
}

fn bareSymbol(alloc: std.mem.Allocator, fn_name: []const u8) ![]const u8 {
    const without_params = if (std.mem.indexOfScalar(u8, fn_name, '(')) |idx| fn_name[0..idx] else fn_name;
    const after_dot = if (std.mem.lastIndexOfScalar(u8, without_params, '.')) |idx| without_params[idx + 1 ..] else without_params;
    return try alloc.dupe(u8, after_dot);
}

pub fn canonicalTermBytes(alloc: std.mem.Allocator, term: provekit.Term) ![]u8 {
    return provekit.jcsStringify(alloc, term);
}

pub fn compileContract(alloc: std.mem.Allocator, contract: FunctionContract) ![]u8 {
    var out = std.ArrayList(u8).empty;
    try out.appendSlice(alloc, "pub fn ");
    try out.appendSlice(alloc, shortFunctionName(contract.fn_name));
    try out.append(alloc, '(');
    for (contract.formals, 0..) |formal, i| {
        if (i != 0) try out.appendSlice(alloc, ", ");
        try out.appendSlice(alloc, formal);
        try out.appendSlice(alloc, ": ");
        try out.appendSlice(alloc, zigTypeForSort(contract.formal_sorts[i]));
    }
    try out.appendSlice(alloc, ") ");
    try out.appendSlice(alloc, zigTypeForSort(contract.return_sort));
    try out.appendSlice(alloc, " {\n");
    try emitTermAsStmt(alloc, &out, contract.bodyTerm(), 1);
    try out.appendSlice(alloc, "}\n");
    return out.toOwnedSlice(alloc);
}

fn emitTermAsStmt(alloc: std.mem.Allocator, out: *std.ArrayList(u8), term: provekit.Term, indent: usize) anyerror!void {
    switch (term) {
        .ctor_term => |ctor| {
            if (std.mem.eql(u8, ctor.name, "zig:seq")) {
                try emitTermAsStmt(alloc, out, ctor.args[0], indent);
                try emitTermAsStmt(alloc, out, ctor.args[1], indent);
            } else if (std.mem.eql(u8, ctor.name, "zig:return")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "return ");
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.appendSlice(alloc, ";\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:decl")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "const ");
                try out.appendSlice(alloc, stringValue(ctor.args[0]));
                try out.appendSlice(alloc, " = ");
                try emitTermAsExpr(alloc, out, ctor.args[1]);
                try out.appendSlice(alloc, ";\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:assign")) {
                try writeIndent(alloc, out, indent);
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.appendSlice(alloc, " = ");
                try emitTermAsExpr(alloc, out, ctor.args[1]);
                try out.appendSlice(alloc, ";\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:if")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "if (");
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.appendSlice(alloc, ") {\n");
                try emitTermAsStmt(alloc, out, ctor.args[1], indent + 1);
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "} else {\n");
                try emitTermAsStmt(alloc, out, ctor.args[2], indent + 1);
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "}\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:while")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "while (");
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.appendSlice(alloc, ") {\n");
                try emitTermAsStmt(alloc, out, ctor.args[1], indent + 1);
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "}\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:for")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "for (");
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.appendSlice(alloc, ") |_| {\n");
                try emitTermAsStmt(alloc, out, ctor.args[1], indent + 1);
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "}\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:break")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "break;\n");
            } else if (std.mem.eql(u8, ctor.name, "zig:continue")) {
                try writeIndent(alloc, out, indent);
                try out.appendSlice(alloc, "continue;\n");
            } else if (!std.mem.eql(u8, ctor.name, "zig:skip")) {
                try writeIndent(alloc, out, indent);
                try emitTermAsExpr(alloc, out, term);
                try out.appendSlice(alloc, ";\n");
            }
        },
        else => {
            try writeIndent(alloc, out, indent);
            try emitTermAsExpr(alloc, out, term);
            try out.appendSlice(alloc, ";\n");
        },
    }
}

fn emitTermAsExpr(alloc: std.mem.Allocator, out: *std.ArrayList(u8), term: provekit.Term) anyerror!void {
    switch (term) {
        .var_term => |v| try out.appendSlice(alloc, v.name),
        .const_term => |c| switch (c.value) {
            .int => |n| try out.print(alloc, "{d}", .{n}),
            .bool => |b| try out.appendSlice(alloc, if (b) "true" else "false"),
            .string => |s| {
                try out.append(alloc, '"');
                try appendEscapedString(alloc, out, s);
                try out.append(alloc, '"');
            },
            .null_void => try out.appendSlice(alloc, "null"),
        },
        .ctor_term => |ctor| {
            const op = ctor.name;
            if (std.mem.eql(u8, op, "zig:add")) return emitBinaryExpr(alloc, out, ctor, "+");
            if (std.mem.eql(u8, op, "zig:sub")) return emitBinaryExpr(alloc, out, ctor, "-");
            if (std.mem.eql(u8, op, "zig:mul")) return emitBinaryExpr(alloc, out, ctor, "*");
            if (std.mem.eql(u8, op, "zig:div")) return emitBinaryExpr(alloc, out, ctor, "/");
            if (std.mem.eql(u8, op, "zig:mod")) return emitBinaryExpr(alloc, out, ctor, "%");
            if (std.mem.eql(u8, op, "zig:eq")) return emitBinaryExpr(alloc, out, ctor, "==");
            if (std.mem.eql(u8, op, "zig:ne")) return emitBinaryExpr(alloc, out, ctor, "!=");
            if (std.mem.eql(u8, op, "zig:lt")) return emitBinaryExpr(alloc, out, ctor, "<");
            if (std.mem.eql(u8, op, "zig:le")) return emitBinaryExpr(alloc, out, ctor, "<=");
            if (std.mem.eql(u8, op, "zig:gt")) return emitBinaryExpr(alloc, out, ctor, ">");
            if (std.mem.eql(u8, op, "zig:ge")) return emitBinaryExpr(alloc, out, ctor, ">=");
            if (std.mem.eql(u8, op, "zig:and")) return emitBinaryExpr(alloc, out, ctor, "and");
            if (std.mem.eql(u8, op, "zig:or")) return emitBinaryExpr(alloc, out, ctor, "or");
            if (std.mem.eql(u8, op, "zig:bitand")) return emitBinaryExpr(alloc, out, ctor, "&");
            if (std.mem.eql(u8, op, "zig:bitor")) return emitBinaryExpr(alloc, out, ctor, "|");
            if (std.mem.eql(u8, op, "zig:bitxor")) return emitBinaryExpr(alloc, out, ctor, "^");
            if (std.mem.eql(u8, op, "zig:shl")) return emitBinaryExpr(alloc, out, ctor, "<<");
            if (std.mem.eql(u8, op, "zig:shr")) return emitBinaryExpr(alloc, out, ctor, ">>");
            if (std.mem.eql(u8, op, "zig:neg")) return emitPrefixExpr(alloc, out, ctor, "-");
            if (std.mem.eql(u8, op, "zig:not")) return emitPrefixExpr(alloc, out, ctor, "!");
            if (std.mem.eql(u8, op, "zig:bitnot")) return emitPrefixExpr(alloc, out, ctor, "~");
            if (std.mem.eql(u8, op, "zig:addr")) return emitPrefixExpr(alloc, out, ctor, "&");
            if (std.mem.eql(u8, op, "zig:deref")) {
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                return out.appendSlice(alloc, ".*");
            }
            if (std.mem.eql(u8, op, "zig:field")) {
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.append(alloc, '.');
                return out.appendSlice(alloc, stringValue(ctor.args[1]));
            }
            if (std.mem.eql(u8, op, "zig:index")) {
                try emitTermAsExpr(alloc, out, ctor.args[0]);
                try out.append(alloc, '[');
                try emitTermAsExpr(alloc, out, ctor.args[1]);
                return out.append(alloc, ']');
            }
            if (std.mem.eql(u8, op, "zig:call")) {
                try out.appendSlice(alloc, stringValue(ctor.args[0]));
                try out.append(alloc, '(');
                for (ctor.args[1..], 0..) |arg, i| {
                    if (i != 0) try out.appendSlice(alloc, ", ");
                    try emitTermAsExpr(alloc, out, arg);
                }
                return out.append(alloc, ')');
            }
            if (std.mem.eql(u8, op, "zig:cast")) {
                try out.appendSlice(alloc, "@as(");
                try out.appendSlice(alloc, stringValue(ctor.args[0]));
                try out.appendSlice(alloc, ", ");
                try emitTermAsExpr(alloc, out, ctor.args[1]);
                return out.append(alloc, ')');
            }
            if (std.mem.eql(u8, op, "zig:unreachable")) return out.appendSlice(alloc, "unreachable");
            if (std.mem.eql(u8, op, "zig:unit")) return out.appendSlice(alloc, "{}");
            try out.appendSlice(alloc, "unreachable");
        },
    }
}

fn emitBinaryExpr(alloc: std.mem.Allocator, out: *std.ArrayList(u8), ctor: provekit.Term.CtorTerm, op: []const u8) !void {
    try out.append(alloc, '(');
    try emitTermAsExpr(alloc, out, ctor.args[0]);
    try out.append(alloc, ' ');
    try out.appendSlice(alloc, op);
    try out.append(alloc, ' ');
    try emitTermAsExpr(alloc, out, ctor.args[1]);
    try out.append(alloc, ')');
}

fn emitPrefixExpr(alloc: std.mem.Allocator, out: *std.ArrayList(u8), ctor: provekit.Term.CtorTerm, op: []const u8) !void {
    try out.append(alloc, '(');
    try out.appendSlice(alloc, op);
    try emitTermAsExpr(alloc, out, ctor.args[0]);
    try out.append(alloc, ')');
}

fn writeIndent(alloc: std.mem.Allocator, out: *std.ArrayList(u8), indent: usize) !void {
    var i: usize = 0;
    while (i < indent) : (i += 1) try out.appendSlice(alloc, "    ");
}

fn appendEscapedString(alloc: std.mem.Allocator, out: *std.ArrayList(u8), value: []const u8) !void {
    for (value) |c| switch (c) {
        '\\' => try out.appendSlice(alloc, "\\\\"),
        '"' => try out.appendSlice(alloc, "\\\""),
        '\n' => try out.appendSlice(alloc, "\\n"),
        '\r' => try out.appendSlice(alloc, "\\r"),
        '\t' => try out.appendSlice(alloc, "\\t"),
        0x20...0x21, 0x23...0x5B, 0x5D...0x7E => try out.append(alloc, c), // printable, not \ or "
        else => {
            // Emit as \xNN for any other byte (control chars, high bytes).
            var buf: [4]u8 = undefined;
            const s = std.fmt.bufPrint(&buf, "\\x{X:0>2}", .{c}) catch unreachable;
            try out.appendSlice(alloc, s);
        },
    };
}

fn stringValue(term: provekit.Term) []const u8 {
    return switch (term) {
        .const_term => |c| switch (c.value) {
            .string => |s| s,
            else => "",
        },
        else => "",
    };
}

fn shortFunctionName(fn_name: []const u8) []const u8 {
    if (std.mem.lastIndexOfScalar(u8, fn_name, '.')) |idx| return fn_name[idx + 1 ..];
    return fn_name;
}

fn zigTypeForSort(sort: provekit.Sort) []const u8 {
    return switch (sort) {
        .primitive => |name| if (std.mem.eql(u8, name, "Bool")) "bool" else if (std.mem.eql(u8, name, "Unit")) "void" else if (std.mem.eql(u8, name, "String")) "[]const u8" else "i32",
        else => "i32",
    };
}

fn trueFormula() provekit.Formula {
    return provekit.Atomic("true", &.{});
}

fn isMutToken(tag: Token.Tag) bool {
    return tag == .keyword_var;
}

fn isIntegerType(text: []const u8) bool {
    if (text.len < 2) return false;
    if (text[0] != 'i' and text[0] != 'u') return false;
    for (text[1..]) |c| if (c < '0' or c > '9') return false;
    return true;
}

fn isIoCallee(name: []const u8) bool {
    return std.mem.eql(u8, name, "std.debug.print") or
        std.mem.startsWith(u8, name, "std.io.") or
        std.mem.startsWith(u8, name, "std.fs.") or
        std.mem.startsWith(u8, name, "std.net.");
}

fn appendGlobal(alloc: std.mem.Allocator, globals: *std.ArrayList(Global), global: Global) !void {
    for (globals.items) |existing| if (std.mem.eql(u8, existing.name, global.name)) return;
    try globals.append(alloc, global);
}

fn appendUniqueString(alloc: std.mem.Allocator, list: *std.ArrayList([]const u8), value: []const u8) !void {
    if (!containsString(list.items, value)) try list.append(alloc, value);
}

fn containsString(list: []const []const u8, value: []const u8) bool {
    for (list) |item| if (std.mem.eql(u8, item, value)) return true;
    return false;
}

fn cloneStringSlice(alloc: std.mem.Allocator, value: []const []const u8) ![]const []const u8 {
    const out = try alloc.alloc([]const u8, value.len);
    for (value, 0..) |item, i| out[i] = item;
    return out;
}

fn termArgs(alloc: std.mem.Allocator, items: anytype) ![]const provekit.Term {
    const info = @typeInfo(@TypeOf(items));
    const len = info.@"struct".fields.len;
    const out = try alloc.alloc(provekit.Term, len);
    inline for (info.@"struct".fields, 0..) |field, i| {
        out[i] = @field(items, field.name);
    }
    return out;
}

test "lifts primitive add function into zig-prefixed source unit and contract" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn add(x: i32, y: i32) i32 {
        \\    return x + y;
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "math.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    try std.testing.expectEqualStrings("<source-unit:math.zig>", out.declarations[0].fn_name);
    try std.testing.expectEqualStrings("math.zig.add", out.declarations[1].fn_name);

    const source_unit_json = try provekit.jcsStringify(alloc, out.declarations[0]);
    try std.testing.expect(std.mem.indexOf(u8, source_unit_json, "\"name\":\"zig:source-unit\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, source_unit_json, "\"name\":\"zig:add\"") != null);
}

test "verify contracts normalize result variable return wrapper arithmetic op and bridge symbol" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn double(x: i64) i64 {
        \\    return x * 2;
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "math.zig");
    const verify = try verifyContracts(alloc, out.declarations);
    try std.testing.expectEqual(@as(usize, 1), verify.len);
    try std.testing.expectEqualStrings("double", verify[0].bridge_source_symbol.?);

    const json = try provekit.jcsStringify(alloc, verify[0]);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"name\":\"result\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"name\":\"*\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"name\":\"zig:return\"") == null);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"bridgeSourceSymbol\":\"double\"") != null);
}

test "refuses unhandled switch without emitting unknown operation" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn f(x: i32) i32 {
        \\    return switch (x) {
        \\        0 => 1,
        \\        else => x,
        \\    };
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "switch.zig");
    try std.testing.expect(out.refusals.len > 0);
    try std.testing.expectEqual(@as(usize, 0), out.declarations.len);
    try std.testing.expectEqualStrings("unhandled-syntax", out.refusals[0].kind);
    try std.testing.expectEqualStrings("switch.zig.f", out.refusals[0].function.?);
}

test "decodes \\n escape in string literal to newline byte in IR term" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn label() []const u8 {
        \\    return "a\nb";
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "strings.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    // declarations[0] is <source-unit>, declarations[1] is the function
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    // The body term of the function should be a zig:return containing a Str with decoded bytes.
    const json = try provekit.jcsStringify(alloc, out.declarations[1].body_term);
    // The JSON encoding of the decoded string "a\nb" (newline byte) must appear, not "a\\nb".
    // In JCS/JSON a newline byte inside a string is encoded as \n (the two-char sequence).
    try std.testing.expect(std.mem.indexOf(u8, json, "\"a\\nb\"") != null);
}

test "decodes \\t and \\r escape sequences in string literals to correct bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn tab() []const u8 {
        \\    return "a\tb";
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "escapes.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    const json = try provekit.jcsStringify(alloc, out.declarations[1].body_term);
    // tab byte encoded as \t in JSON
    try std.testing.expect(std.mem.indexOf(u8, json, "\"a\\tb\"") != null);
}

test "decodes \\x hex escape in string literal to raw byte" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn hex() []const u8 {
        \\    return "\x41bc";
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "hex.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    const json = try provekit.jcsStringify(alloc, out.declarations[1].body_term);
    // \x41 == 'A', so decoded bytes are "Abc"
    try std.testing.expect(std.mem.indexOf(u8, json, "\"Abc\"") != null);
}

test "decodes \\u{NNNN} unicode escape in string literal to UTF-8 bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn uni() []const u8 {
        \\    return "\u{263A}";
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "unicode.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    const json = try provekit.jcsStringify(alloc, out.declarations[1].body_term);
    // U+263A is ☺, UTF-8: E2 98 BA
    try std.testing.expect(std.mem.indexOf(u8, json, "\u{263A}") != null);
}

test "decodes \\\\ escaped backslash in string literal to single backslash byte" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn bs() []const u8 {
        \\    return "a\\b";
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "backslash.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    const json = try provekit.jcsStringify(alloc, out.declarations[1].body_term);
    // single backslash encoded in JSON as \\
    try std.testing.expect(std.mem.indexOf(u8, json, "\"a\\\\b\"") != null);
}

test "lifts multiline string literal as raw decoded bytes without escape processing" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn multi() []const u8 {
        \\    return
        \\        \\hello
        \\        \\world
        \\    ;
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "multi.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    try std.testing.expectEqual(@as(usize, 2), out.declarations.len);
    const json = try provekit.jcsStringify(alloc, out.declarations[1].body_term);
    // multiline string "hello\nworld" -- newline between lines encoded as \n in JSON
    try std.testing.expect(std.mem.indexOf(u8, json, "\"hello\\nworld\"") != null);
}

test "round trip string literal with escape sequences preserves decoded bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn label() []const u8 {
        \\    return "a\nb";
        \\}
        \\
    ;

    const first = try liftSource(alloc, src, "rt_escape.zig");
    try std.testing.expectEqual(@as(usize, 0), first.refusals.len);
    const compiled = try compileContract(alloc, first.declarations[1]);
    const second = try liftSource(alloc, compiled, "rt_escape.zig");
    try std.testing.expectEqual(@as(usize, 0), second.refusals.len);

    const first_bytes = try canonicalTermBytes(alloc, first.declarations[1].bodyTerm());
    const second_bytes = try canonicalTermBytes(alloc, second.declarations[1].bodyTerm());
    try std.testing.expectEqualStrings(first_bytes, second_bytes);
}

test "sorts canonical effects and hashes opaque loop term" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\var counter: i32 = 0;
        \\
        \\pub fn tick(limit: i32) i32 {
        \\    while (counter < limit) {
        \\        counter = counter + 1;
        \\    }
        \\    return counter;
        \\}
        \\
    ;

    const out = try liftSource(alloc, src, "counter.zig");
    try std.testing.expectEqual(@as(usize, 0), out.refusals.len);
    const json = try provekit.jcsStringify(alloc, out.declarations[1]);
    const reads_pos = std.mem.indexOf(u8, json, "\"kind\":\"reads\"") orelse return error.MissingReads;
    const writes_pos = std.mem.indexOf(u8, json, "\"kind\":\"writes\"") orelse return error.MissingWrites;
    const loop_pos = std.mem.indexOf(u8, json, "\"kind\":\"opaque_loop\"") orelse return error.MissingOpaqueLoop;
    try std.testing.expect(reads_pos < writes_pos);
    try std.testing.expect(writes_pos < loop_pos);
    try std.testing.expect(std.mem.indexOf(u8, json, "\"loopCid\":\"blake3-512:") != null);
}

test "round trip compile then lift preserves canonical body term bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn add(x: i32, y: i32) i32 {
        \\    return x + y;
        \\}
        \\
    ;

    const first = try liftSource(alloc, src, "math.zig");
    try std.testing.expectEqual(@as(usize, 0), first.refusals.len);
    const compiled = try compileContract(alloc, first.declarations[1]);
    const second = try liftSource(alloc, compiled, "math.zig");
    try std.testing.expectEqual(@as(usize, 0), second.refusals.len);

    const first_bytes = try canonicalTermBytes(alloc, first.declarations[1].bodyTerm());
    const second_bytes = try canonicalTermBytes(alloc, second.declarations[1].bodyTerm());
    try std.testing.expectEqualStrings(first_bytes, second_bytes);
}

test "round trip compile then lift preserves canonical loop body term bytes" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn scan(items: []const i32, flag: bool) i32 {
        \\    for (items) |_| {
        \\        if (flag) {
        \\            break;
        \\        }
        \\        continue;
        \\    }
        \\    return 0;
        \\}
        \\
    ;

    const first = try liftSource(alloc, src, "loops.zig");
    try std.testing.expectEqual(@as(usize, 0), first.refusals.len);
    const compiled = try compileContract(alloc, first.declarations[1]);
    const second = try liftSource(alloc, compiled, "loops.zig");
    try std.testing.expectEqual(@as(usize, 0), second.refusals.len);

    const first_bytes = try canonicalTermBytes(alloc, first.declarations[1].bodyTerm());
    const second_bytes = try canonicalTermBytes(alloc, second.declarations[1].bodyTerm());
    try std.testing.expectEqualStrings(first_bytes, second_bytes);

    const first_hash = try provekit.jcsHash(alloc, first_bytes);
    const second_hash = try provekit.jcsHash(alloc, second_bytes);
    try std.testing.expectEqualStrings(first_hash, second_hash);
}

test "sugar body emits ast template alongside body text" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\pub fn fetch(url: []const u8, headers: []const u8) i32 {
        \\    return http.get(url, headers);
        \\}
        \\
    ;

    const body = (try sugarBodySourceForFunc(alloc, src, "shim.zig", "fetch")).?;
    try std.testing.expectEqualStrings("return http.get(url, headers);", body.body_text);
    try std.testing.expect(std.mem.startsWith(u8, body.source_cid, "blake3-512:"));
    try std.testing.expect(std.mem.startsWith(u8, body.template_cid, "blake3-512:"));
    try std.testing.expectEqualStrings("url", body.param_names[0]);
    try std.testing.expectEqualStrings("headers", body.param_names[1]);

    const template_json = try provekit.jcsStringify(alloc, body.ast_template);
    try std.testing.expect(std.mem.indexOf(u8, template_json, "\"kind\":\"block\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, template_json, "\"kind\":\"method_call\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, template_json, "\"method\":\"get\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, template_json, "\"kind\":\"param_ref\"") != null);
}

test "sugar body template cid is stable under parameter renaming" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src_a =
        \\pub fn fetch(url: []const u8, headers: []const u8) i32 {
        \\    return http.get(url, headers);
        \\}
        \\
    ;
    const src_b =
        \\pub fn fetch(uri: []const u8, hdrs: []const u8) i32 {
        \\    return http.get(uri, hdrs);
        \\}
        \\
    ;

    const body_a = (try sugarBodySourceForFunc(alloc, src_a, "a.zig", "fetch")).?;
    const body_b = (try sugarBodySourceForFunc(alloc, src_b, "b.zig", "fetch")).?;
    try std.testing.expectEqualStrings(body_a.template_cid, body_b.template_cid);
    try std.testing.expect(!std.mem.eql(u8, body_a.body_text, body_b.body_text));
    try std.testing.expect(!std.mem.eql(u8, body_a.source_cid, body_b.source_cid));
}

test "recognize emits exact tag for alpha-equivalent user function" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const sugar =
        \\pub fn fetch(url: []const u8, headers: []const u8) i32 {
        \\    return http.get(url, headers);
        \\}
        \\
    ;
    const sugar_body = (try sugarBodySourceForFunc(alloc, sugar, "shim.zig", "fetch")).?;
    const binding = BindingTemplate{
        .concept_name = "concept:http-request",
        .library_tag = "provekit-shim-zig-http",
        .family = "concept:family:http",
        .ast_template = sugar_body.ast_template,
        .template_cid = sugar_body.template_cid,
        .param_names = sugar_body.param_names,
        .contract_cid = "blake3-512:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    };

    const response = try recognizeSourceText(
        alloc,
        "src/client.zig",
        \\pub fn fetchUrl(uri: []const u8, hdrs: []const u8) i32 {
        \\    return http.get(uri, hdrs);
        \\}
        \\
    ,
        &.{binding},
    );
    try std.testing.expectEqual(@as(usize, 1), response.tags.len);
    const tag = response.tags[0];
    try std.testing.expectEqualStrings("src/client.zig", tag.file);
    try std.testing.expectEqualStrings("fetchUrl", tag.function_name);
    try std.testing.expectEqualStrings("concept:http-request", tag.concept_name.?);
    try std.testing.expectEqualStrings("provekit-shim-zig-http", tag.library_tag.?);
    try std.testing.expectEqualStrings("concept:family:http", tag.family.?);
    try std.testing.expectEqualStrings(sugar_body.template_cid, tag.template_cid);
    try std.testing.expectEqualStrings("exact", tag.match_tier);
    try std.testing.expectEqualStrings("uri", tag.param_bindings[0].source_text);
    try std.testing.expectEqualStrings("hdrs", tag.param_bindings[1].source_text);
}

test "recognize returns empty tags for non matching source" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const sugar =
        \\pub fn parse(input: []const u8) i32 {
        \\    return json.parse(input);
        \\}
        \\
    ;
    const sugar_body = (try sugarBodySourceForFunc(alloc, sugar, "shim.zig", "parse")).?;
    const binding = BindingTemplate{
        .concept_name = "concept:json-parse",
        .library_tag = "json",
        .ast_template = sugar_body.ast_template,
        .template_cid = sugar_body.template_cid,
        .param_names = sugar_body.param_names,
    };

    const response = try recognizeSourceText(
        alloc,
        "client.zig",
        \\pub fn parse(input: []const u8) i32 {
        \\    return other.parse(input);
        \\}
        \\
    ,
        &.{binding},
    );
    try std.testing.expectEqual(@as(usize, 0), response.tags.len);
}

test "recognize routes multiple bindings per call site pool" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const http_body = (try sugarBodySourceForFunc(
        alloc,
        \\pub fn fetch(url: []const u8, headers: []const u8) i32 {
        \\    return http.get(url, headers);
        \\}
        \\
    ,
        "http.zig",
        "fetch",
    )).?;
    const sql_body = (try sugarBodySourceForFunc(
        alloc,
        \\pub fn query(conn: Db, sql: []const u8, args: Args) Result {
        \\    return conn.execute(sql, args);
        \\}
        \\
    ,
        "sql.zig",
        "query",
    )).?;

    const bindings = [_]BindingTemplate{
        .{
            .concept_name = "concept:http-request",
            .library_tag = "http-lib",
            .family = "concept:family:http",
            .ast_template = http_body.ast_template,
            .template_cid = http_body.template_cid,
            .param_names = http_body.param_names,
        },
        .{
            .concept_name = "concept:sql-execute",
            .library_tag = "sql-lib",
            .family = "concept:family:sql",
            .ast_template = sql_body.ast_template,
            .template_cid = sql_body.template_cid,
            .param_names = sql_body.param_names,
        },
    };

    const response = try recognizeSourceText(
        alloc,
        "calls.zig",
        \\pub fn fetchUrl(uri: []const u8, hdrs: []const u8) i32 {
        \\    return http.get(uri, hdrs);
        \\}
        \\
        \\pub fn runQuery(db: Db, query_text: []const u8, params: Args) Result {
        \\    return db.execute(query_text, params);
        \\}
        \\
    ,
        &bindings,
    );
    try std.testing.expectEqual(@as(usize, 2), response.tags.len);

    var saw_http = false;
    var saw_sql = false;
    for (response.tags) |tag| {
        if (std.mem.eql(u8, tag.concept_name.?, "concept:http-request")) {
            saw_http = true;
            try std.testing.expectEqualStrings("fetchUrl", tag.function_name);
        }
        if (std.mem.eql(u8, tag.concept_name.?, "concept:sql-execute")) {
            saw_sql = true;
            try std.testing.expectEqualStrings("runQuery", tag.function_name);
        }
    }
    try std.testing.expect(saw_http);
    try std.testing.expect(saw_sql);
}
