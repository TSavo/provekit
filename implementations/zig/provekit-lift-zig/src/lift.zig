// provekit-lift-zig/src/lift.zig
//
// Pure parsing logic: no IO.  Imported by provekit-lift-zig (the CLI binary)
// and by provekit-lsp-zig (the LSP plugin).

const std = @import("std");
const provekit = @import("provekit-ir");

pub const Annotation = struct {
    function_name: []const u8,
    kind: Kind,
    target_cid: ?[]const u8 = null,
    line: usize,

    pub const Kind = enum {
        contract,
        implement,
        verify,
    };
};

pub const ImplicationDecl = struct {
    name: []const u8,
    antecedent: []const u8,
    consequent: []const u8,
    antecedent_slot: []const u8 = "inv",
    consequent_slot: []const u8 = "inv",
    prover: []const u8,
    proof_witness: []const u8 = "",

    pub fn jsonStringify(self: ImplicationDecl, jws: anytype) !void {
        try jws.beginObject();
        try jws.objectField("name");
        try jws.write(self.name);
        try jws.objectField("antecedent");
        try jws.write(self.antecedent);
        try jws.objectField("consequent");
        try jws.write(self.consequent);
        try jws.objectField("antecedentSlot");
        try jws.write(self.antecedent_slot);
        try jws.objectField("consequentSlot");
        try jws.write(self.consequent_slot);
        try jws.objectField("prover");
        try jws.write(self.prover);
        try jws.objectField("proofWitness");
        try jws.write(self.proof_witness);
        try jws.endObject();
    }
};

pub const LiftOutput = struct {
    declarations: []provekit.Decl,
    implications: []ImplicationDecl,
};

/// Parse provekit annotations from Zig source text.
/// Caller owns the returned slice; call `alloc.free(slice)` when done.
pub fn parseAnnotations(alloc: std.mem.Allocator, text: []const u8) ![]Annotation {
    var annotations: std.ArrayList(Annotation) = .empty;
    errdefer annotations.deinit(alloc);

    var lines = std.mem.splitScalar(u8, text, '\n');
    var line_num: usize = 0;
    while (lines.next()) |line| : (line_num += 1) {
        const trimmed = std.mem.trim(u8, line, " \t");

        if (std.mem.startsWith(u8, trimmed, "//provekit:implement ")) {
            const cid = std.mem.trim(u8, trimmed[20..], " \t");
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(alloc, .{
                .function_name = fn_name,
                .kind = .implement,
                .target_cid = cid,
                .line = line_num,
            });
        } else if (std.mem.startsWith(u8, trimmed, "//provekit:contract")) {
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(alloc, .{
                .function_name = fn_name,
                .kind = .contract,
                .line = line_num,
            });
        } else if (std.mem.startsWith(u8, trimmed, "//provekit:verify")) {
            const fn_name = findAheadFnName(text, line_num);
            try annotations.append(alloc, .{
                .function_name = fn_name,
                .kind = .verify,
                .line = line_num,
            });
        }
    }

    return annotations.toOwnedSlice(alloc);
}

fn findAheadFnName(text: []const u8, start_line: usize) []const u8 {
    var lines = std.mem.splitScalar(u8, text, '\n');
    var current: usize = 0;
    // Recognize Zig function prefixes (review feedback: PR #165 / CodeRabbit):
    // bare `fn`, `pub fn`, `export fn`, `extern fn`, `inline fn`, plus
    // combinations such as `pub export fn`, `pub extern fn`, `pub inline fn`.
    // Strip leading visibility/linkage qualifiers until we find `fn `.
    const qualifiers = [_][]const u8{ "pub", "export", "extern", "inline", "noinline", "comptime" };
    while (lines.next()) |line| : (current += 1) {
        if (current <= start_line) continue;
        if (current > start_line + 10) break;

        var trimmed = std.mem.trim(u8, line, " \t");
        // Strip qualifiers iteratively. Each pass strips one qualifier (or an
        // `extern "C"`-style calling-convention quoted string) so combinations
        // like `pub extern "C" fn` are handled.
        var stripped = true;
        while (stripped) {
            stripped = false;
            for (qualifiers) |q| {
                if (trimmed.len > q.len and
                    std.mem.startsWith(u8, trimmed, q) and
                    (trimmed[q.len] == ' ' or trimmed[q.len] == '\t'))
                {
                    trimmed = std.mem.trimStart(u8, trimmed[q.len..], " \t");
                    stripped = true;
                    break;
                }
            }
            // Tolerate `extern "C"` calling-convention spec.
            if (trimmed.len > 0 and trimmed[0] == '"') {
                if (std.mem.indexOfScalar(u8, trimmed[1..], '"')) |close_idx| {
                    const after = trimmed[1 + close_idx + 1 ..];
                    trimmed = std.mem.trimStart(u8, after, " \t");
                    stripped = true;
                }
            }
        }
        if (std.mem.startsWith(u8, trimmed, "fn ")) {
            const after = trimmed[3..];
            const end = std.mem.indexOfAny(u8, after, " (\n") orelse after.len;
            return after[0..end];
        }
    }
    return "unknown";
}

/// Lift annotations from source text into a slice of provekit IR declarations.
/// Caller owns the returned slice.
pub fn liftToDecls(alloc: std.mem.Allocator, text: []const u8) ![]provekit.Decl {
    const anns = try parseAnnotations(alloc, text);
    defer alloc.free(anns);

    var decls: std.ArrayList(provekit.Decl) = .empty;
    errdefer decls.deinit(alloc);

    for (anns) |ann| {
        switch (ann.kind) {
            .contract => {
                const post_args = [_]provekit.Term{};
                const post = provekit.Atomic("true", &post_args);
                try decls.append(alloc, .{ .contract = .{
                    .name = ann.function_name,
                    .post = post,
                } });
            },
            .implement => {
                if (ann.target_cid) |cid| {
                    try decls.append(alloc, .{ .bridge = .{
                        .name = ann.function_name,
                        .source_symbol = ann.function_name,
                        .source_layer = "zig",
                        .source_contract_cid = "",
                        .target_contract_cid = cid,
                        .target_proof_cid = "",
                        .target_layer = "rust",
                    } });
                }
            },
            .verify => {},
        }
    }

    return decls.toOwnedSlice(alloc);
}

pub fn liftSource(alloc: std.mem.Allocator, text: []const u8, source_file: []const u8) !LiftOutput {
    var declarations: std.ArrayList(provekit.Decl) = .empty;
    var implications: std.ArrayList(ImplicationDecl) = .empty;

    const annotation_decls = try liftToDecls(alloc, text);
    for (annotation_decls) |decl| try declarations.append(alloc, decl);

    var functions: std.ArrayList(FunctionDef) = .empty;
    var tests: std.ArrayList(TestBlock) = .empty;
    try collectTopLevelBlocks(alloc, text, &functions, &tests);

    try liftZigTests(alloc, functions.items, tests.items, source_file, &declarations, &implications);
    try liftProductionWalk(alloc, functions.items, source_file, &declarations, &implications);

    return .{
        .declarations = try declarations.toOwnedSlice(alloc),
        .implications = try implications.toOwnedSlice(alloc),
    };
}

const SourceLine = struct {
    text: []const u8,
    number: usize,
};

const FunctionDef = struct {
    name: []const u8,
    params: []const []const u8,
    body: []const SourceLine,
    start_line: usize,
};

const TestBlock = struct {
    name: []const u8,
    body: []const SourceLine,
    start_line: usize,
};

const FunctionPrecondition = struct {
    name: []const u8,
    params: []const []const u8,
    precondition: provekit.Formula,
};

const ConditionFrame = struct {
    formula: provekit.Formula,
    depth: isize,
};

const CallsiteHit = struct {
    line: usize,
    col: usize,
    stmt_index: usize,
    args: []const provekit.Term,
    conditions: []const provekit.Formula,
};

const Binding = struct {
    name: []const u8,
    term: provekit.Term,
};

const ObservedCall = struct {
    local: []const u8,
    base: []const u8,
    term: provekit.Term,
};

fn collectTopLevelBlocks(
    alloc: std.mem.Allocator,
    text: []const u8,
    functions: *std.ArrayList(FunctionDef),
    tests: *std.ArrayList(TestBlock),
) !void {
    var lines = std.mem.splitScalar(u8, text, '\n');
    var line_no: usize = 1;
    while (lines.next()) |line| : (line_no += 1) {
        const trimmed = std.mem.trim(u8, line, " \t");
        if (parseFnSignature(alloc, trimmed)) |sig| {
            const body = try collectBody(alloc, &lines, &line_no, line);
            try functions.append(alloc, .{
                .name = sig.name,
                .params = sig.params,
                .body = body,
                .start_line = line_no,
            });
        } else if (parseTestName(trimmed)) |test_name| {
            const body = try collectBody(alloc, &lines, &line_no, line);
            try tests.append(alloc, .{
                .name = test_name,
                .body = body,
                .start_line = line_no,
            });
        }
    }
}

const FnSignature = struct {
    name: []const u8,
    params: []const []const u8,
};

fn parseFnSignature(alloc: std.mem.Allocator, line: []const u8) ?FnSignature {
    var rest = stripFnQualifiers(line);
    if (!std.mem.startsWith(u8, rest, "fn ")) return null;
    rest = rest[3..];
    const open = std.mem.indexOfScalar(u8, rest, '(') orelse return null;
    const close = std.mem.indexOfScalarPos(u8, rest, open + 1, ')') orelse return null;
    return .{
        .name = std.mem.trim(u8, rest[0..open], " \t"),
        .params = parseParamNames(alloc, rest[open + 1 .. close]) catch return null,
    };
}

fn stripFnQualifiers(line: []const u8) []const u8 {
    const qualifiers = [_][]const u8{ "pub", "export", "extern", "inline", "noinline", "comptime" };
    var trimmed = std.mem.trim(u8, line, " \t");
    var changed = true;
    while (changed) {
        changed = false;
        for (qualifiers) |q| {
            if (trimmed.len > q.len and std.mem.startsWith(u8, trimmed, q) and (trimmed[q.len] == ' ' or trimmed[q.len] == '\t')) {
                trimmed = std.mem.trimStart(u8, trimmed[q.len..], " \t");
                changed = true;
                break;
            }
        }
        if (trimmed.len > 0 and trimmed[0] == '"') {
            if (std.mem.indexOfScalar(u8, trimmed[1..], '"')) |close_idx| {
                trimmed = std.mem.trimStart(u8, trimmed[1 + close_idx + 1 ..], " \t");
                changed = true;
            }
        }
    }
    return trimmed;
}

fn parseParamNames(alloc: std.mem.Allocator, params_text: []const u8) ![]const []const u8 {
    var params: std.ArrayList([]const u8) = .empty;
    var parts = std.mem.splitScalar(u8, params_text, ',');
    while (parts.next()) |part| {
        const trimmed = std.mem.trim(u8, part, " \t");
        if (trimmed.len == 0) continue;
        const colon = std.mem.indexOfScalar(u8, trimmed, ':') orelse continue;
        try params.append(alloc, std.mem.trim(u8, trimmed[0..colon], " \t"));
    }
    return params.toOwnedSlice(alloc);
}

fn parseTestName(line: []const u8) ?[]const u8 {
    if (!std.mem.startsWith(u8, line, "test ")) return null;
    const first = std.mem.indexOfScalar(u8, line, '"') orelse return "";
    const second_rel = std.mem.indexOfScalar(u8, line[first + 1 ..], '"') orelse return "";
    return line[first + 1 .. first + 1 + second_rel];
}

fn collectBody(
    alloc: std.mem.Allocator,
    lines: *std.mem.SplitIterator(u8, .scalar),
    line_no: *usize,
    header_line: []const u8,
) ![]const SourceLine {
    var body: std.ArrayList(SourceLine) = .empty;
    var depth = braceDelta(header_line);
    while (depth > 0) {
        const maybe_line = lines.next() orelse break;
        line_no.* += 1;
        const next_depth = depth + braceDelta(maybe_line);
        if (!(next_depth <= 0 and onlyClosesBlock(maybe_line))) {
            try body.append(alloc, .{ .text = maybe_line, .number = line_no.* });
        }
        depth = next_depth;
    }
    return body.toOwnedSlice(alloc);
}

fn braceDelta(line: []const u8) isize {
    var delta: isize = 0;
    for (line) |c| {
        if (c == '{') delta += 1;
        if (c == '}') delta -= 1;
    }
    return delta;
}

fn onlyClosesBlock(line: []const u8) bool {
    const trimmed = std.mem.trim(u8, line, " \t\r\n;");
    return std.mem.eql(u8, trimmed, "}");
}

fn liftProductionWalk(
    alloc: std.mem.Allocator,
    functions: []const FunctionDef,
    source_file: []const u8,
    declarations: *std.ArrayList(provekit.Decl),
    implications: *std.ArrayList(ImplicationDecl),
) !void {
    var preconditions: std.ArrayList(FunctionPrecondition) = .empty;
    for (functions) |function| {
        if (try liftFunctionPrecondition(alloc, function)) |pre| {
            try preconditions.append(alloc, pre);
        }
    }

    var used_names: std.ArrayList([]const u8) = .empty;
    for (functions) |caller| {
        for (preconditions.items) |callee| {
            if (std.mem.eql(u8, caller.name, callee.name)) continue;
            try emitWalksForCallee(alloc, caller, callee, source_file, declarations, implications, &used_names);
        }
    }
}

fn liftFunctionPrecondition(alloc: std.mem.Allocator, function: FunctionDef) !?FunctionPrecondition {
    var formulas: std.ArrayList(provekit.Formula) = .empty;
    for (function.body, 0..) |line, idx| {
        const trimmed = std.mem.trim(u8, line.text, " \t");
        if (extractAssertCondition(trimmed)) |condition| {
            try formulas.append(alloc, try liftFormula(alloc, condition));
        } else if (extractIfCondition(trimmed)) |condition| {
            if (lineReturnsError(trimmed) or (idx + 1 < function.body.len and lineReturnsError(function.body[idx + 1].text))) {
                try formulas.append(alloc, try liftNegatedFormula(alloc, condition));
            }
        }
    }
    if (formulas.items.len == 0) return null;
    return .{
        .name = function.name,
        .params = function.params,
        .precondition = try combineAnd(alloc, formulas.items),
    };
}

fn extractAssertCondition(line: []const u8) ?[]const u8 {
    if (std.mem.indexOf(u8, line, "std.debug.assert(")) |pos| {
        return extractDelimited(line, pos + "std.debug.assert".len);
    }
    return null;
}

fn extractIfCondition(line: []const u8) ?[]const u8 {
    const pos = std.mem.indexOf(u8, line, "if ") orelse return null;
    return extractDelimited(line, pos + 3);
}

fn extractDelimited(line: []const u8, open_pos: usize) ?[]const u8 {
    if (open_pos >= line.len or line[open_pos] != '(') return null;
    var depth: isize = 0;
    var i = open_pos;
    while (i < line.len) : (i += 1) {
        if (line[i] == '(') depth += 1;
        if (line[i] == ')') {
            depth -= 1;
            if (depth == 0) return std.mem.trim(u8, line[open_pos + 1 .. i], " \t");
        }
    }
    return null;
}

fn lineReturnsError(line: []const u8) bool {
    return std.mem.indexOf(u8, line, "return error.") != null;
}

fn emitWalksForCallee(
    alloc: std.mem.Allocator,
    caller: FunctionDef,
    callee: FunctionPrecondition,
    source_file: []const u8,
    declarations: *std.ArrayList(provekit.Decl),
    implications: *std.ArrayList(ImplicationDecl),
    used_names: *std.ArrayList([]const u8),
) !void {
    const hits = try findCallsites(alloc, caller, callee.name);
    for (hits) |hit| {
        if (hit.args.len != callee.params.len) continue;

        var wp = callee.precondition;
        for (callee.params, 0..) |param, i| {
            wp = try substituteFormula(alloc, wp, param, hit.args[i]);
        }
        if (hit.conditions.len > 0) {
            const premise = try combineAnd(alloc, hit.conditions);
            const operands = try formulaArgs(alloc, .{ premise, wp });
            wp = provekit.Implies(operands);
        }

        const base = try std.fmt.allocPrint(alloc, "{s}@{s}:{d}:{d}", .{ callee.name, source_file, hit.line, hit.col });
        const callsite_name = try std.fmt.allocPrint(alloc, "{s}::callsite", .{base});
        try appendEdge(alloc, declarations, implications, used_names, callsite_name, wp, wp, caller.name, callee.name, "zig-wp-walk");

        var previous_wp = wp;
        var i = hit.stmt_index;
        while (i > 0) {
            i -= 1;
            if (try bindingFromLine(alloc, caller.body[i].text)) |binding| {
                const next_wp = try substituteFormula(alloc, previous_wp, binding.name, binding.term);
                const let_name = try std.fmt.allocPrint(alloc, "{s}::let:{s}", .{ base, binding.name });
                try appendEdge(alloc, declarations, implications, used_names, let_name, next_wp, previous_wp, caller.name, callee.name, "zig-wp-walk");
                previous_wp = next_wp;
            }
        }

        const entry_name = try std.fmt.allocPrint(alloc, "{s}::entry", .{base});
        try appendEdge(alloc, declarations, implications, used_names, entry_name, previous_wp, previous_wp, caller.name, callee.name, "zig-wp-walk");
    }
}

fn findCallsites(alloc: std.mem.Allocator, caller: FunctionDef, callee_name: []const u8) ![]const CallsiteHit {
    var hits: std.ArrayList(CallsiteHit) = .empty;
    var conditions: std.ArrayList(ConditionFrame) = .empty;
    var depth: isize = 0;

    const needle = try std.fmt.allocPrint(alloc, "{s}(", .{callee_name});
    for (caller.body, 0..) |line, idx| {
        while (conditions.items.len > 0 and conditions.items[conditions.items.len - 1].depth > depth) {
            _ = conditions.pop();
        }

        const maybe_condition = if (extractIfCondition(line.text)) |condition| try liftFormula(alloc, condition) else null;
        if (std.mem.indexOf(u8, line.text, needle)) |pos| {
            const args = try callArgsAt(alloc, line.text, pos + callee_name.len);
            var conds: std.ArrayList(provekit.Formula) = .empty;
            for (conditions.items) |condition| try conds.append(alloc, condition.formula);
            if (maybe_condition) |condition| try conds.append(alloc, condition);
            try hits.append(alloc, .{
                .line = line.number,
                .col = pos + 1,
                .stmt_index = idx,
                .args = args,
                .conditions = try conds.toOwnedSlice(alloc),
            });
        }

        if (maybe_condition) |condition| {
            if (std.mem.indexOfScalar(u8, line.text, '{') != null) {
                try conditions.append(alloc, .{ .formula = condition, .depth = depth + 1 });
            }
        }
        depth += braceDelta(line.text);
    }

    return hits.toOwnedSlice(alloc);
}

fn callArgsAt(alloc: std.mem.Allocator, line: []const u8, open_paren: usize) ![]const provekit.Term {
    const args_text = extractDelimited(line, open_paren) orelse "";
    const arg_slices = try splitArgs(alloc, args_text);
    var args: std.ArrayList(provekit.Term) = .empty;
    for (arg_slices) |arg| {
        if (arg.len == 0) continue;
        try args.append(alloc, try termFromExpr(alloc, arg));
    }
    return args.toOwnedSlice(alloc);
}

fn appendEdge(
    alloc: std.mem.Allocator,
    declarations: *std.ArrayList(provekit.Decl),
    implications: *std.ArrayList(ImplicationDecl),
    used_names: *std.ArrayList([]const u8),
    raw_name: []const u8,
    pre: provekit.Formula,
    post: provekit.Formula,
    caller_name: []const u8,
    callee_name: []const u8,
    prover: []const u8,
) !void {
    const name = try uniqueName(alloc, used_names, raw_name);
    try declarations.append(alloc, .{ .contract = .{
        .name = name,
        .out_binding = "result",
        .pre = pre,
        .post = post,
    } });
    try implications.append(alloc, .{
        .name = try std.fmt.allocPrint(alloc, "{s}::pre-implies-post", .{name}),
        .antecedent = name,
        .consequent = name,
        .antecedent_slot = "pre",
        .consequent_slot = "post",
        .prover = prover,
        .proof_witness = try std.fmt.allocPrint(alloc, "{s}->{s}", .{ caller_name, callee_name }),
    });
}

fn liftZigTests(
    alloc: std.mem.Allocator,
    functions: []const FunctionDef,
    tests: []const TestBlock,
    source_file: []const u8,
    declarations: *std.ArrayList(provekit.Decl),
    implications: *std.ArrayList(ImplicationDecl),
) !void {
    var used_names: std.ArrayList([]const u8) = .empty;
    for (tests) |test_block| {
        var observed: std.ArrayList(ObservedCall) = .empty;
        for (test_block.body) |line| {
            if (try observedCallBinding(alloc, line, functions, source_file)) |call| {
                try observed.append(alloc, call);
                continue;
            }
            if (std.mem.indexOf(u8, line.text, "std.testing.expectEqual(")) |pos| {
                const args_text = extractDelimited(line.text, pos + "std.testing.expectEqual".len) orelse continue;
                const args = try splitArgs(alloc, args_text);
                if (args.len < 2) continue;
                if (findObservedByLocal(observed.items, args[1])) |call| {
                    const expected = try termFromExpr(alloc, args[0]);
                    const inv_args = try termArgs(alloc, .{ call.term, expected });
                    const assertion = provekit.Atomic("=", inv_args);
                    try appendTestValueScope(alloc, declarations, implications, &used_names, call, assertion, test_block.name);
                }
            } else if (std.mem.indexOf(u8, line.text, "std.testing.expect(")) |pos| {
                const expr = extractDelimited(line.text, pos + "std.testing.expect".len) orelse continue;
                if (try assertionFormulaForObserved(alloc, expr, observed.items)) |assertion| {
                    const call = findObservedInExpr(expr, observed.items) orelse continue;
                    try appendTestValueScope(alloc, declarations, implications, &used_names, call, assertion, test_block.name);
                }
            }
        }
    }
}

fn observedCallBinding(
    alloc: std.mem.Allocator,
    line: SourceLine,
    functions: []const FunctionDef,
    source_file: []const u8,
) !?ObservedCall {
    const binding = (try bindingFromLine(alloc, line.text)) orelse return null;
    const call_name = callNameFromExpr(binding.term) orelse return null;
    if (!isKnownFunction(functions, call_name)) return null;
    const needle = try std.fmt.allocPrint(alloc, "{s}(", .{call_name});
    const col = (std.mem.indexOf(u8, line.text, needle) orelse 0) + 1;
    return .{
        .local = binding.name,
        .base = try std.fmt.allocPrint(alloc, "{s}@{s}:{d}:{d}", .{ call_name, source_file, line.number, col }),
        .term = binding.term,
    };
}

fn appendTestValueScope(
    alloc: std.mem.Allocator,
    declarations: *std.ArrayList(provekit.Decl),
    implications: *std.ArrayList(ImplicationDecl),
    used_names: *std.ArrayList([]const u8),
    call: ObservedCall,
    assertion: provekit.Formula,
    test_name: []const u8,
) !void {
    const facts_name = try uniqueName(alloc, used_names, try std.fmt.allocPrint(alloc, "{s}::facts", .{call.base}));
    const assertion_name = try uniqueName(alloc, used_names, try std.fmt.allocPrint(alloc, "{s}::assertion", .{call.base}));
    const fact_args = try termArgs(alloc, .{ provekit.Var(call.local), call.term });
    const facts = provekit.Atomic("=", fact_args);
    try declarations.append(alloc, .{ .contract = .{ .name = facts_name, .inv = facts } });
    try declarations.append(alloc, .{ .contract = .{ .name = assertion_name, .inv = assertion } });
    try implications.append(alloc, .{
        .name = try uniqueName(alloc, used_names, try std.fmt.allocPrint(alloc, "{s}::facts-implies-assertion", .{call.base})),
        .antecedent = facts_name,
        .consequent = assertion_name,
        .antecedent_slot = "inv",
        .consequent_slot = "inv",
        .prover = "zig-test-value-scope",
        .proof_witness = try std.fmt.allocPrint(alloc, "{s} assertion", .{test_name}),
    });
}

fn assertionFormulaForObserved(alloc: std.mem.Allocator, expr: []const u8, observed: []const ObservedCall) !?provekit.Formula {
    const call = findObservedInExpr(expr, observed) orelse return null;
    const ops = [_]struct { text: []const u8, name: []const u8 }{
        .{ .text = "==", .name = "=" },
        .{ .text = "!=", .name = "≠" },
        .{ .text = ">=", .name = "≥" },
        .{ .text = "<=", .name = "≤" },
        .{ .text = ">", .name = ">" },
        .{ .text = "<", .name = "<" },
    };
    const trimmed = std.mem.trim(u8, expr, " \t");
    for (ops) |op| {
        if (std.mem.indexOf(u8, trimmed, op.text)) |pos| {
            const left = std.mem.trim(u8, trimmed[0..pos], " \t");
            const right = std.mem.trim(u8, trimmed[pos + op.text.len ..], " \t");
            const lhs = if (std.mem.eql(u8, left, call.local)) call.term else try termFromExpr(alloc, left);
            const rhs = if (std.mem.eql(u8, right, call.local)) call.term else try termFromExpr(alloc, right);
            const args = try termArgs(alloc, .{ lhs, rhs });
            return provekit.Atomic(op.name, args);
        }
    }
    return null;
}

fn findObservedInExpr(expr: []const u8, observed: []const ObservedCall) ?ObservedCall {
    for (observed) |call| {
        if (std.mem.indexOf(u8, expr, call.local) != null) return call;
    }
    return null;
}

fn findObservedByLocal(observed: []const ObservedCall, local: []const u8) ?ObservedCall {
    const trimmed = std.mem.trim(u8, local, " \t");
    for (observed) |call| {
        if (std.mem.eql(u8, call.local, trimmed)) return call;
    }
    return null;
}

fn isKnownFunction(functions: []const FunctionDef, name: []const u8) bool {
    for (functions) |function| {
        if (std.mem.eql(u8, function.name, name)) return true;
    }
    return false;
}

fn callNameFromExpr(term: provekit.Term) ?[]const u8 {
    return switch (term) {
        .ctor_term => |ctor| ctor.name,
        else => null,
    };
}

fn bindingFromLine(alloc: std.mem.Allocator, line: []const u8) !?Binding {
    const trimmed = std.mem.trim(u8, line, " \t");
    const start = if (std.mem.startsWith(u8, trimmed, "const "))
        "const ".len
    else if (std.mem.startsWith(u8, trimmed, "var "))
        "var ".len
    else
        return null;
    const rest = trimmed[start..];
    const eq = std.mem.indexOfScalar(u8, rest, '=') orelse return null;
    const name_part = std.mem.trim(u8, rest[0..eq], " \t");
    const name_end = std.mem.indexOfAny(u8, name_part, ": \t") orelse name_part.len;
    const name = name_part[0..name_end];
    var value = std.mem.trim(u8, rest[eq + 1 ..], " \t;");
    if (std.mem.startsWith(u8, value, "try ")) value = std.mem.trimStart(u8, value[4..], " \t");
    return .{ .name = name, .term = try termFromExpr(alloc, value) };
}

fn liftFormula(alloc: std.mem.Allocator, expr: []const u8) !provekit.Formula {
    return liftFormulaWithOps(alloc, expr, false);
}

fn liftNegatedFormula(alloc: std.mem.Allocator, expr: []const u8) !provekit.Formula {
    return liftFormulaWithOps(alloc, expr, true);
}

fn liftFormulaWithOps(alloc: std.mem.Allocator, expr: []const u8, negate: bool) !provekit.Formula {
    const ops = [_]struct { text: []const u8, name: []const u8, inverse: []const u8 }{
        .{ .text = ">=", .name = "≥", .inverse = "<" },
        .{ .text = "<=", .name = "≤", .inverse = ">" },
        .{ .text = "==", .name = "=", .inverse = "≠" },
        .{ .text = "!=", .name = "≠", .inverse = "=" },
        .{ .text = ">", .name = ">", .inverse = "≤" },
        .{ .text = "<", .name = "<", .inverse = "≥" },
    };
    const trimmed = trimOuterParens(std.mem.trim(u8, expr, " \t"));
    for (ops) |op| {
        if (std.mem.indexOf(u8, trimmed, op.text)) |pos| {
            const left = std.mem.trim(u8, trimmed[0..pos], " \t");
            const right = std.mem.trim(u8, trimmed[pos + op.text.len ..], " \t");
            const args = try termArgs(alloc, .{ try termFromExpr(alloc, left), try termFromExpr(alloc, right) });
            return provekit.Atomic(if (negate) op.inverse else op.name, args);
        }
    }
    const args = try termArgs(alloc, .{ try termFromExpr(alloc, trimmed), provekit.BoolConst(!negate) });
    return provekit.Atomic("=", args);
}

fn termFromExpr(alloc: std.mem.Allocator, expr: []const u8) !provekit.Term {
    var trimmed = std.mem.trim(u8, expr, " \t;");
    if (std.mem.startsWith(u8, trimmed, "try ")) trimmed = std.mem.trimStart(u8, trimmed[4..], " \t");
    trimmed = stripAsCast(trimmed);

    if (std.fmt.parseInt(i64, trimmed, 10)) |n| {
        return provekit.Num(n);
    } else |_| {}
    if (std.mem.eql(u8, trimmed, "true")) return provekit.BoolConst(true);
    if (std.mem.eql(u8, trimmed, "false")) return provekit.BoolConst(false);
    if (trimmed.len >= 2 and trimmed[0] == '"' and trimmed[trimmed.len - 1] == '"') {
        return provekit.Str(trimmed[1 .. trimmed.len - 1]);
    }

    if (std.mem.indexOfScalar(u8, trimmed, '(')) |open| {
        if (trimmed.len > 0 and trimmed[trimmed.len - 1] == ')') {
            const name = std.mem.trim(u8, trimmed[0..open], " \t");
            const args_text = trimmed[open + 1 .. trimmed.len - 1];
            const arg_slices = try splitArgs(alloc, args_text);
            var args: std.ArrayList(provekit.Term) = .empty;
            for (arg_slices) |arg| {
                if (arg.len == 0) continue;
                try args.append(alloc, try termFromExpr(alloc, arg));
            }
            return provekit.Ctor(name, try args.toOwnedSlice(alloc));
        }
    }

    return provekit.Var(trimmed);
}

fn stripAsCast(expr: []const u8) []const u8 {
    const trimmed = std.mem.trim(u8, expr, " \t");
    if (!std.mem.startsWith(u8, trimmed, "@as(") or trimmed[trimmed.len - 1] != ')') return trimmed;
    const inner = trimmed[4 .. trimmed.len - 1];
    const comma = std.mem.indexOfScalar(u8, inner, ',') orelse return trimmed;
    return std.mem.trim(u8, inner[comma + 1 ..], " \t");
}

fn trimOuterParens(expr: []const u8) []const u8 {
    var trimmed = expr;
    while (trimmed.len >= 2 and trimmed[0] == '(' and trimmed[trimmed.len - 1] == ')') {
        trimmed = std.mem.trim(u8, trimmed[1 .. trimmed.len - 1], " \t");
    }
    return trimmed;
}

fn splitArgs(alloc: std.mem.Allocator, args_text: []const u8) ![]const []const u8 {
    var args: std.ArrayList([]const u8) = .empty;
    var depth: isize = 0;
    var start: usize = 0;
    for (args_text, 0..) |c, i| {
        if (c == '(') depth += 1;
        if (c == ')') depth -= 1;
        if (c == ',' and depth == 0) {
            try args.append(alloc, std.mem.trim(u8, args_text[start..i], " \t"));
            start = i + 1;
        }
    }
    const tail = std.mem.trim(u8, args_text[start..], " \t");
    if (tail.len > 0) try args.append(alloc, tail);
    return args.toOwnedSlice(alloc);
}

fn combineAnd(alloc: std.mem.Allocator, formulas: []const provekit.Formula) !provekit.Formula {
    if (formulas.len == 1) return formulas[0];
    return provekit.And(try cloneFormulaSlice(alloc, formulas));
}

fn substituteFormula(
    alloc: std.mem.Allocator,
    formula: provekit.Formula,
    name: []const u8,
    replacement: provekit.Term,
) anyerror!provekit.Formula {
    return switch (formula) {
        .atomic => |a| provekit.Atomic(a.name, try substituteTermSlice(alloc, a.args, name, replacement)),
        .connective => |c| .{ .connective = .{
            .kind = c.kind,
            .operands = try substituteFormulaSlice(alloc, c.operands, name, replacement),
        } },
        .quantifier => formula,
    };
}

fn substituteFormulaSlice(
    alloc: std.mem.Allocator,
    formulas: []const provekit.Formula,
    name: []const u8,
    replacement: provekit.Term,
) anyerror![]const provekit.Formula {
    var out = try alloc.alloc(provekit.Formula, formulas.len);
    for (formulas, 0..) |formula, i| out[i] = try substituteFormula(alloc, formula, name, replacement);
    return out;
}

fn substituteTermSlice(
    alloc: std.mem.Allocator,
    terms: []const provekit.Term,
    name: []const u8,
    replacement: provekit.Term,
) anyerror![]const provekit.Term {
    var out = try alloc.alloc(provekit.Term, terms.len);
    for (terms, 0..) |term, i| out[i] = try substituteTerm(alloc, term, name, replacement);
    return out;
}

fn substituteTerm(
    alloc: std.mem.Allocator,
    term: provekit.Term,
    name: []const u8,
    replacement: provekit.Term,
) anyerror!provekit.Term {
    return switch (term) {
        .var_term => |v| if (std.mem.eql(u8, v.name, name)) replacement else term,
        .ctor_term => |c| provekit.Ctor(c.name, try substituteTermSlice(alloc, c.args, name, replacement)),
        else => term,
    };
}

fn cloneFormulaSlice(alloc: std.mem.Allocator, formulas: []const provekit.Formula) ![]const provekit.Formula {
    const out = try alloc.alloc(provekit.Formula, formulas.len);
    @memcpy(out, formulas);
    return out;
}

fn formulaArgs(alloc: std.mem.Allocator, items: anytype) ![]const provekit.Formula {
    const info = @typeInfo(@TypeOf(items));
    const len = info.@"struct".fields.len;
    var out = try alloc.alloc(provekit.Formula, len);
    inline for (info.@"struct".fields, 0..) |field, i| {
        out[i] = @field(items, field.name);
    }
    return out;
}

fn termArgs(alloc: std.mem.Allocator, items: anytype) ![]const provekit.Term {
    const info = @typeInfo(@TypeOf(items));
    const len = info.@"struct".fields.len;
    var out = try alloc.alloc(provekit.Term, len);
    inline for (info.@"struct".fields, 0..) |field, i| {
        out[i] = @field(items, field.name);
    }
    return out;
}

fn uniqueName(alloc: std.mem.Allocator, used_names: *std.ArrayList([]const u8), raw_name: []const u8) ![]const u8 {
    if (!containsName(used_names.items, raw_name)) {
        try used_names.append(alloc, raw_name);
        return raw_name;
    }
    var i: usize = 1;
    while (true) : (i += 1) {
        const candidate = try std.fmt.allocPrint(alloc, "{s}::{d}", .{ raw_name, i });
        if (!containsName(used_names.items, candidate)) {
            try used_names.append(alloc, candidate);
            return candidate;
        }
    }
}

fn containsName(names: []const []const u8, needle: []const u8) bool {
    for (names) |name| {
        if (std.mem.eql(u8, name, needle)) return true;
    }
    return false;
}

test "parseAnnotations finds contract" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\fn myFn(x: i32) void {
        \\    _ = x;
        \\}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqual(Annotation.Kind.contract, anns[0].kind);
    try std.testing.expectEqualStrings("myFn", anns[0].function_name);
}

test "parseAnnotations finds implement" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:implement blake3-512:abc123
        \\fn bridge(x: i32) void {
        \\    _ = x;
        \\}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqual(Annotation.Kind.implement, anns[0].kind);
    try std.testing.expectEqualStrings("blake3-512:abc123", anns[0].target_cid.?);
}

test "parseAnnotations finds verify" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:verify
        \\fn checkFn() void {}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqual(Annotation.Kind.verify, anns[0].kind);
    try std.testing.expectEqualStrings("checkFn", anns[0].function_name);
}

test "parseAnnotations empty source" {
    const alloc = std.testing.allocator;
    const anns = try parseAnnotations(alloc, "");
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 0), anns.len);
}

test "liftToDecls contract produces IR" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\fn myFn() void {}
    ;
    const decls = try liftToDecls(alloc, src);
    defer alloc.free(decls);
    try std.testing.expectEqual(@as(usize, 1), decls.len);
    switch (decls[0]) {
        .contract => |c| try std.testing.expectEqualStrings("myFn", c.name),
        else => return error.WrongKind,
    }
}

test "liftSource walks production callsite preconditions back to entry" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\fn checked(x: i32) !i32 {
        \\    if (x < 10) return error.TooSmall;
        \\    return x;
        \\}
        \\
        \\fn composedOk() !i32 {
        \\    const y = 42;
        \\    return checked(y);
        \\}
    ;

    const out = try liftSource(alloc, src, "app.zig");

    try std.testing.expectEqual(@as(usize, 3), out.implications.len);
    for (out.implications) |imp| {
        try std.testing.expectEqualStrings("zig-wp-walk", imp.prover);
        try std.testing.expectEqualStrings("pre", imp.antecedent_slot);
        try std.testing.expectEqualStrings("post", imp.consequent_slot);
    }

    const let_edge = findContractWithSuffix(out.declarations, "::let:y") orelse return error.MissingLetEdge;
    try std.testing.expect(std.mem.startsWith(u8, let_edge.name, "checked@app.zig:"));

    const pre_json = try provekit.jcsStringify(alloc, let_edge.pre.?);
    const post_json = try provekit.jcsStringify(alloc, let_edge.post.?);
    try std.testing.expect(std.mem.indexOf(u8, pre_json, "\"name\":\"≥\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, pre_json, "\"value\":42") != null);
    try std.testing.expect(std.mem.indexOf(u8, pre_json, "\"value\":10") != null);
    try std.testing.expect(std.mem.indexOf(u8, post_json, "\"name\":\"y\"") != null);

    const entry = findContractWithSuffix(out.declarations, "::entry") orelse return error.MissingEntryEdge;
    const entry_pre_json = try provekit.jcsStringify(alloc, entry.pre.?);
    try std.testing.expectEqualStrings(pre_json, entry_pre_json);
}

test "liftSource shows production composes while Zig unit tests conflict" {
    var arena = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const src =
        \\const std = @import("std");
        \\
        \\fn checked(x: i32) !i32 {
        \\    if (x < 10) return error.TooSmall;
        \\    return x;
        \\}
        \\
        \\fn composedOk() !i32 {
        \\    const y = 42;
        \\    return checked(y);
        \\}
        \\
        \\test "checked returns 42" {
        \\    const actual = try checked(42);
        \\    try std.testing.expectEqual(@as(i32, 42), actual);
        \\}
        \\
        \\test "checked does not return 42" {
        \\    const actual = try checked(42);
        \\    try std.testing.expect(actual != 42);
        \\}
    ;

    const out = try liftSource(alloc, src, "app.zig");

    try std.testing.expectEqual(@as(usize, 3), countContractsWithSuffix(out.declarations, "::callsite") + countContractsWithSuffix(out.declarations, "::let:y") + countContractsWithSuffix(out.declarations, "::entry"));
    try std.testing.expect(findContractWithSuffix(out.declarations, "::let:y") != null);

    var eq_count: usize = 0;
    var neq_count: usize = 0;
    var assertion_count: usize = 0;
    for (out.declarations) |decl| {
        switch (decl) {
            .contract => |contract| {
                if (!std.mem.startsWith(u8, contract.name, "checked@app.zig:")) continue;
                if (std.mem.endsWith(u8, contract.name, "::assertion")) {
                    assertion_count += 1;
                    const inv = contract.inv.?;
                    switch (inv) {
                        .atomic => |a| {
                            if (std.mem.eql(u8, a.name, "=")) eq_count += 1;
                            if (std.mem.eql(u8, a.name, "≠")) neq_count += 1;
                        },
                        else => return error.ExpectedAtomicAssertion,
                    }
                }
                try std.testing.expect(std.mem.indexOf(u8, contract.name, "checked returns 42") == null);
                try std.testing.expect(std.mem.indexOf(u8, contract.name, "checked does not return 42") == null);
            },
            else => {},
        }
    }
    try std.testing.expectEqual(@as(usize, 2), assertion_count);
    try std.testing.expectEqual(@as(usize, 1), eq_count);
    try std.testing.expectEqual(@as(usize, 1), neq_count);

    try std.testing.expectEqual(@as(usize, 3), countImplicationsByProver(out.implications, "zig-wp-walk"));
    try std.testing.expectEqual(@as(usize, 2), countImplicationsByProver(out.implications, "zig-test-value-scope"));
}

fn findContractWithSuffix(decls: []const provekit.Decl, suffix: []const u8) ?provekit.Decl.ContractDecl {
    for (decls) |decl| {
        switch (decl) {
            .contract => |contract| {
                if (std.mem.endsWith(u8, contract.name, suffix)) return contract;
            },
            else => {},
        }
    }
    return null;
}

fn countContractsWithSuffix(decls: []const provekit.Decl, suffix: []const u8) usize {
    var count: usize = 0;
    for (decls) |decl| {
        switch (decl) {
            .contract => |contract| {
                if (std.mem.endsWith(u8, contract.name, suffix)) count += 1;
            },
            else => {},
        }
    }
    return count;
}

fn countImplicationsByProver(implications: []const ImplicationDecl, prover: []const u8) usize {
    var count: usize = 0;
    for (implications) |imp| {
        if (std.mem.eql(u8, imp.prover, prover)) count += 1;
    }
    return count;
}

// Zig function-prefix coverage (review feedback: PR #165 / CodeRabbit).
// findAheadFnName must handle visibility/linkage qualifiers preceding `fn`.

test "parseAnnotations finds pub fn" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\pub fn pubFn(x: i32) void {
        \\    _ = x;
        \\}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqualStrings("pubFn", anns[0].function_name);
}

test "parseAnnotations finds export fn" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\export fn exportedFn() void {}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqualStrings("exportedFn", anns[0].function_name);
}

test "parseAnnotations finds extern fn" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\extern fn externFn() void;
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqualStrings("externFn", anns[0].function_name);
}

test "parseAnnotations finds inline fn" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\inline fn inlinedFn() void {}
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqualStrings("inlinedFn", anns[0].function_name);
}

test "parseAnnotations finds pub extern \"C\" fn" {
    const alloc = std.testing.allocator;
    const src =
        \\//provekit:contract
        \\pub extern "C" fn cAbiFn() void;
    ;
    const anns = try parseAnnotations(alloc, src);
    defer alloc.free(anns);
    try std.testing.expectEqual(@as(usize, 1), anns.len);
    try std.testing.expectEqualStrings("cAbiFn", anns[0].function_name);
}
