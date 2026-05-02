const std = @import("std");

// provekit-ir — Zig kit for ProvekIt protocol v1.1.0
//
// JCS canonical JSON: all object keys emitted in strict alphabetical order.
// String escaping matches RFC 8785 (zig std.json default with escape_unicode=false).
// Hashing: BLAKE3-512 via std.crypto.blake3 (64-byte XOF output).

// ---------------------------------------------------------------------------
// Sort
// ---------------------------------------------------------------------------

pub const Sort = union(enum) {
    primitive: []const u8,

    pub const Bool = Sort{ .primitive = "Bool" };
    pub const Int = Sort{ .primitive = "Int" };
    pub const Real = Sort{ .primitive = "Real" };
    pub const String = Sort{ .primitive = "String" };
    pub const Ref = Sort{ .primitive = "Ref" };

    pub fn jsonStringify(self: Sort, jws: anytype) !void {
        switch (self) {
            .primitive => |name| {
                try jws.beginObject();
                try jws.objectField("kind");
                try jws.write("primitive");
                try jws.objectField("name");
                try jws.write(name);
                try jws.endObject();
            },
        }
    }
};

// ---------------------------------------------------------------------------
// Term
// ---------------------------------------------------------------------------

pub const Term = union(enum) {
    var_term: VarTerm,
    const_term: ConstTerm,
    ctor_term: CtorTerm,

    pub const VarTerm = struct {
        name: []const u8,
    };

    pub const ConstTerm = struct {
        value: ConstValue,
        sort: Sort,
    };

    pub const CtorTerm = struct {
        name: []const u8,
        args: []const Term,
    };

    pub const ConstValue = union(enum) {
        int: i64,
        string: []const u8,
        bool: bool,
        null_void: void,

        pub fn jsonStringify(self: ConstValue, jws: anytype) !void {
            switch (self) {
                .int => |v| try jws.write(v),
                .string => |v| try jws.write(v),
                .bool => |v| try jws.write(v),
                .null_void => try jws.write(null),
            }
        }
    };

    pub fn jsonStringify(self: Term, jws: anytype) !void {
        try jws.beginObject();
        switch (self) {
            .var_term => |t| {
                try jws.objectField("kind");
                try jws.write("var");
                try jws.objectField("name");
                try jws.write(t.name);
            },
            .const_term => |t| {
                try jws.objectField("kind");
                try jws.write("const");
                try jws.objectField("sort");
                try jws.write(t.sort);
                try jws.objectField("value");
                try jws.write(t.value);
            },
            .ctor_term => |t| {
                try jws.objectField("args");
                try jws.write(t.args);
                try jws.objectField("kind");
                try jws.write("ctor");
                try jws.objectField("name");
                try jws.write(t.name);
            },
        }
        try jws.endObject();
    }
};

// ---------------------------------------------------------------------------
// Formula
// ---------------------------------------------------------------------------

pub const Formula = union(enum) {
    atomic: AtomicFormula,
    connective: ConnectiveFormula,
    quantifier: QuantifierFormula,

    pub const AtomicFormula = struct {
        name: []const u8,
        args: []const Term,
    };

    pub const ConnectiveFormula = struct {
        kind: ConnectiveKind,
        operands: []const Formula,
    };

    pub const ConnectiveKind = enum {
        @"and",
        @"or",
        @"not",
        @"implies",

        pub fn jsonStringify(self: ConnectiveKind, jws: anytype) !void {
            const str = switch (self) {
                .@"and" => "and",
                .@"or" => "or",
                .@"not" => "not",
                .@"implies" => "implies",
            };
            try jws.write(str);
        }
    };

    pub const QuantifierFormula = struct {
        kind: QuantifierKind,
        name: []const u8,
        sort: Sort,
        body: *const Formula,
    };

    pub const QuantifierKind = enum {
        forall,
        exists,

        pub fn jsonStringify(self: QuantifierKind, jws: anytype) !void {
            const str = switch (self) {
                .forall => "forall",
                .exists => "exists",
            };
            try jws.write(str);
        }
    };

    pub fn jsonStringify(self: Formula, jws: anytype) !void {
        try jws.beginObject();
        switch (self) {
            .atomic => |f| {
                try jws.objectField("args");
                try jws.write(f.args);
                try jws.objectField("kind");
                try jws.write("atomic");
                try jws.objectField("name");
                try jws.write(f.name);
            },
            .connective => |f| {
                try jws.objectField("kind");
                try jws.write(f.kind);
                try jws.objectField("operands");
                try jws.write(f.operands);
            },
            .quantifier => |f| {
                try jws.objectField("body");
                try jws.write(f.body.*);
                try jws.objectField("kind");
                try jws.write(f.kind);
                try jws.objectField("name");
                try jws.write(f.name);
                try jws.objectField("sort");
                try jws.write(f.sort);
            },
        }
        try jws.endObject();
    }
};

// ---------------------------------------------------------------------------
// Declaration
// ---------------------------------------------------------------------------

pub const Decl = union(enum) {
    contract: ContractDecl,
    bridge: BridgeDecl,

    pub const ContractDecl = struct {
        name: []const u8,
        out_binding: []const u8 = "out",
        pre: ?Formula = null,
        post: ?Formula = null,
        inv: ?Formula = null,
    };

    pub const BridgeDecl = struct {
        name: []const u8,
        source_symbol: []const u8,
        source_layer: []const u8,
        source_contract_cid: []const u8,
        target_contract_cid: []const u8,
        target_proof_cid: []const u8,
        target_layer: []const u8,
        notes: ?[]const u8 = null,
    };

    pub fn jsonStringify(self: Decl, jws: anytype) !void {
        switch (self) {
            .contract => |d| {
                try jws.beginObject();
                try jws.objectField("kind");
                try jws.write("contract");
                try jws.objectField("name");
                try jws.write(d.name);
                try jws.objectField("outBinding");
                try jws.write(d.out_binding);
                if (d.pre) |pre| {
                    try jws.objectField("pre");
                    try jws.write(pre);
                }
                if (d.post) |post| {
                    try jws.objectField("post");
                    try jws.write(post);
                }
                if (d.inv) |inv| {
                    try jws.objectField("inv");
                    try jws.write(inv);
                }
                try jws.endObject();
            },
            .bridge => |d| {
                try jws.beginObject();
                try jws.objectField("kind");
                try jws.write("bridge");
                try jws.objectField("name");
                try jws.write(d.name);
                if (d.notes) |notes| {
                    try jws.objectField("notes");
                    try jws.write(notes);
                }
                try jws.objectField("sourceContractCid");
                try jws.write(d.source_contract_cid);
                try jws.objectField("sourceLayer");
                try jws.write(d.source_layer);
                try jws.objectField("sourceSymbol");
                try jws.write(d.source_symbol);
                try jws.objectField("targetContractCid");
                try jws.write(d.target_contract_cid);
                try jws.objectField("targetLayer");
                try jws.write(d.target_layer);
                try jws.objectField("targetProofCid");
                try jws.write(d.target_proof_cid);
                try jws.endObject();
            },
        }
    }
};

// ---------------------------------------------------------------------------
// Convenience constructors
// ---------------------------------------------------------------------------

pub fn Var(name: []const u8) Term {
    return .{ .var_term = .{ .name = name } };
}

pub fn Num(n: i64) Term {
    return .{ .const_term = .{ .value = .{ .int = n }, .sort = Sort.Int } };
}

pub fn Str(s: []const u8) Term {
    return .{ .const_term = .{ .value = .{ .string = s }, .sort = Sort.String } };
}

pub fn BoolConst(b: bool) Term {
    return .{ .const_term = .{ .value = .{ .bool = b }, .sort = Sort.Bool } };
}

pub fn Null() Term {
    return .{ .const_term = .{ .value = .{ .null_void = {} }, .sort = Sort.Ref } };
}

pub fn Ctor(name: []const u8, args: []const Term) Term {
    return .{ .ctor_term = .{ .name = name, .args = args } };
}

pub fn Atomic(name: []const u8, args: []const Term) Formula {
    return .{ .atomic = .{ .name = name, .args = args } };
}

pub fn And(operands: []const Formula) Formula {
    return .{ .connective = .{ .kind = .@"and", .operands = operands } };
}

pub fn Or(operands: []const Formula) Formula {
    return .{ .connective = .{ .kind = .@"or", .operands = operands } };
}

pub fn Not(operands: []const Formula) Formula {
    return .{ .connective = .{ .kind = .@"not", .operands = operands } };
}

pub fn Implies(operands: []const Formula) Formula {
    return .{ .connective = .{ .kind = .@"implies", .operands = operands } };
}

pub fn Forall(name: []const u8, sort_: Sort, body: *const Formula) Formula {
    return .{ .quantifier = .{ .kind = .forall, .name = name, .sort = sort_, .body = body } };
}

pub fn Exists(name: []const u8, sort_: Sort, body: *const Formula) Formula {
    return .{ .quantifier = .{ .kind = .exists, .name = name, .sort = sort_, .body = body } };
}

// ---------------------------------------------------------------------------
// JCS + Hash helpers
// ---------------------------------------------------------------------------

pub fn jcsStringify(alloc: std.mem.Allocator, value: anytype) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, value, .{ .whitespace = .minified });
}

pub fn jcsHash(alloc: std.mem.Allocator, jcs_bytes: []const u8) ![]u8 {
    var hash_out: [64]u8 = undefined;
    var hasher = std.crypto.hash.Blake3.init(.{});
    hasher.update(jcs_bytes);
    hasher.final(&hash_out);

    const prefix = "blake3-512:";
    const hex = std.fmt.bytesToHex(hash_out, .lower);
    var result = try alloc.alloc(u8, prefix.len + hex.len);
    @memcpy(result[0..prefix.len], prefix);
    @memcpy(result[prefix.len..], &hex);
    return result;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test "eq atomic JCS matches Rust" {
    const alloc = std.testing.allocator;

    const ctor_args = [_]Term{Str("42")};
    const lhs = Ctor("parse_int", &ctor_args);
    const rhs = Num(42);
    const atomic_args = [_]Term{ lhs, rhs };
    const f = Atomic("=", &atomic_args);

    const jcs = try jcsStringify(alloc, f);
    defer alloc.free(jcs);

    const expected =
        "{\"args\":[{\"args\":[{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"},\"value\":\"42\"}],\"kind\":\"ctor\",\"name\":\"parse_int\"},"
        ++ "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":42}],"
        ++ "\"kind\":\"atomic\",\"name\":\"=\"}";

    try std.testing.expectEqualStrings(expected, jcs);
}

test "eq atomic hash matches Rust" {
    const alloc = std.testing.allocator;

    const ctor_args = [_]Term{Str("42")};
    const lhs = Ctor("parse_int", &ctor_args);
    const rhs = Num(42);
    const atomic_args = [_]Term{ lhs, rhs };
    const f = Atomic("=", &atomic_args);

    const jcs = try jcsStringify(alloc, f);
    defer alloc.free(jcs);

    const hash = try jcsHash(alloc, jcs);
    defer alloc.free(hash);

    const expected =
        "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18"
        ++ "539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa";

    try std.testing.expectEqualStrings(expected, hash);
}

test "pattern1 bounded loop JCS matches Rust" {
    const alloc = std.testing.allocator;

    const x1 = Var("x");
    const x2 = Var("x");
    const x3 = Var("x");
    const zero1 = Num(0);
    const zero2 = Num(0);
    const hundred = Num(100);

    const lower_args = [_]Term{ x1, zero1 };
    const lower = Atomic("≥", &lower_args);

    const upper_args = [_]Term{ x2, hundred };
    const upper = Atomic("<", &upper_args);

    const conj_args = [_]Formula{ lower, upper };
    const antecedent = And(&conj_args);

    const inner_args = [_]Term{ x3, zero2 };
    const inner = Atomic("≥", &inner_args);

    const impl_args = [_]Formula{ antecedent, inner };
    const body = Implies(&impl_args);

    const q = Forall("x", Sort.Int, &body);

    const jcs = try jcsStringify(alloc, q);
    defer alloc.free(jcs);

    const expected =
        "{\"body\":{\"kind\":\"implies\",\"operands\":[{\"kind\":\"and\",\"operands\":[{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},"
        ++ "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],\"kind\":\"atomic\",\"name\":\"≥\"},"
        ++ "{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":100}],"
        ++ "\"kind\":\"atomic\",\"name\":\"<\"}]},{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},"
        ++ "{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],\"kind\":\"atomic\",\"name\":\"≥\"}]},"
        ++ "\"kind\":\"forall\",\"name\":\"x\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";

    try std.testing.expectEqualStrings(expected, jcs);
}

test "contract decl JCS" {
    const alloc = std.testing.allocator;

    const x = Var("x");
    const zero = Num(0);
    const pre_args = [_]Term{ x, zero };
    const pre = Atomic("≥", &pre_args);
    const d = Decl{ .contract = .{
        .name = "parseInt",
        .out_binding = "out",
        .pre = pre,
    } };

    const jcs = try jcsStringify(alloc, d);
    defer alloc.free(jcs);

    const expected =
        "{\"kind\":\"contract\",\"name\":\"parseInt\",\"outBinding\":\"out\","
        ++ "\"pre\":{\"args\":[{\"kind\":\"var\",\"name\":\"x\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"},\"value\":0}],"
        ++ "\"kind\":\"atomic\",\"name\":\"≥\"}}";

    try std.testing.expectEqualStrings(expected, jcs);
}

test "bridge decl JCS" {
    const alloc = std.testing.allocator;

    const d = Decl{ .bridge = .{
        .name = "myBridge",
        .source_symbol = "source",
        .source_layer = "c-kit",
        .source_contract_cid = "bafySource",
        .target_contract_cid = "bafyTarget",
        .target_proof_cid = "bafyProof",
        .target_layer = "coq",
        .notes = "some notes",
    } };

    const jcs = try jcsStringify(alloc, d);
    defer alloc.free(jcs);

    const expected =
        "{\"kind\":\"bridge\",\"name\":\"myBridge\",\"notes\":\"some notes\","
        ++ "\"sourceContractCid\":\"bafySource\",\"sourceLayer\":\"c-kit\",\"sourceSymbol\":\"source\","
        ++ "\"targetContractCid\":\"bafyTarget\",\"targetLayer\":\"coq\",\"targetProofCid\":\"bafyProof\"}";

    try std.testing.expectEqualStrings(expected, jcs);
}
