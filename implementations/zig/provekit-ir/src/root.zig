const std = @import("std");

pub const Sort = union(enum) {
    primitive: []const u8,
    set: *const Sort,
    tuple: []const Sort,
    function: FunctionSort,

    pub const FunctionSort = struct {
        domain: []const Sort,
        range: Sort,
    };

    pub fn jsonStringify(self: Sort, jws: anytype) !void {
        try jws.beginObject();
        switch (self) {
            .primitive => |name| {
                try jws.objectField("kind");
                try jws.write("primitive");
                try jws.objectField("name");
                try jws.write(name);
            },
            .set => |element| {
                try jws.objectField("kind");
                try jws.write("set");
                try jws.objectField("element");
                try jws.write(element.*);
            },
            .tuple => |elements| {
                try jws.objectField("kind");
                try jws.write("tuple");
                try jws.objectField("elements");
                try jws.write(elements);
            },
            .function => |f| {
                try jws.objectField("kind");
                try jws.write("function");
                try jws.objectField("domain");
                try jws.write(f.domain);
                try jws.objectField("range");
                try jws.write(f.range);
            },
        }
        try jws.endObject();
    }

    pub const Bool = Sort{ .primitive = "Bool" };
    pub const Int = Sort{ .primitive = "Int" };
    pub const Real = Sort{ .primitive = "Real" };
    pub const String = Sort{ .primitive = "String" };
    pub const Ref = Sort{ .primitive = "Ref" };
    pub const Node = Sort{ .primitive = "Node" };
    pub const Edge = Sort{ .primitive = "Edge" };
};

pub const Term = union(enum) {
    var_term: VarTerm,
    const_term: ConstTerm,
    ctor_term: CtorTerm,
    lambda_term: LambdaTerm,
    let_term: LetTerm,

    pub const VarTerm = struct {
        name: []const u8,
        sort: Sort,
    };

    pub const ConstTerm = struct {
        value: Value,
        sort: Sort,
    };

    pub const CtorTerm = struct {
        name: []const u8,
        args: []const Term,
        sort: Sort,
    };

    pub const LambdaTerm = struct {
        param_name: []const u8,
        param_sort: Sort,
        body: *const Term,
        sort: Sort,
    };

    pub const LetTerm = struct {
        bindings: []const LetBinding,
        body: *const Term,
        sort: Sort,
    };

    pub const LetBinding = struct {
        name: []const u8,
        bound_term: Term,
    };

    pub const Value = union(enum) {
        int: i64,
        string: []const u8,
        bool: bool,
        real: f64,

        pub fn jsonStringify(self: Value, jws: anytype) !void {
            switch (self) {
                .int => |v| try jws.write(v),
                .string => |v| try jws.write(v),
                .bool => |v| try jws.write(v),
                .real => |v| try jws.write(v),
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
                try jws.objectField("value");
                try jws.write(t.value);
                try jws.objectField("sort");
                try jws.write(t.sort);
            },
            .ctor_term => |t| {
                try jws.objectField("kind");
                try jws.write("ctor");
                try jws.objectField("name");
                try jws.write(t.name);
                try jws.objectField("args");
                try jws.write(t.args);
            },
            .lambda_term => |t| {
                try jws.objectField("kind");
                try jws.write("lambda");
                try jws.objectField("paramName");
                try jws.write(t.param_name);
                try jws.objectField("paramSort");
                try jws.write(t.param_sort);
                try jws.objectField("body");
                try jws.write(t.body.*);
            },
            .let_term => |t| {
                try jws.objectField("kind");
                try jws.write("let");
                try jws.objectField("bindings");
                try jws.beginArray();
                for (t.bindings) |b| {
                    try jws.arrayElem();
                    try jws.beginObject();
                    try jws.objectField("name");
                    try jws.write(b.name);
                    try jws.objectField("boundTerm");
                    try jws.write(b.bound_term);
                    try jws.endObject();
                }
                try jws.endArray();
                try jws.objectField("body");
                try jws.write(t.body.*);
            },
        }
        try jws.endObject();
    }
};

pub const Formula = union(enum) {
    atomic: AtomicFormula,
    connective: ConnectiveFormula,
    quantifier: QuantifierFormula,
    choice: ChoiceFormula,

    pub const AtomicFormula = struct {
        name: []const u8,
        args: []const Term,
    };

    pub const ConnectiveFormula = struct {
        kind: ConnectiveKind,
        operands: []const Formula,
    };

    pub const ConnectiveKind = enum {
        and,
        or,
        not,
        implies,

        pub fn jsonStringify(self: ConnectiveKind, jws: anytype) !void {
            const str = switch (self) {
                .and => "and",
                .or => "or",
                .not => "not",
                .implies => "implies",
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

    pub const ChoiceFormula = struct {
        var_name: []const u8,
        sort: Sort,
        body: *const Formula,
    };

    pub fn jsonStringify(self: Formula, jws: anytype) !void {
        try jws.beginObject();
        switch (self) {
            .atomic => |f| {
                try jws.objectField("kind");
                try jws.write("atomic");
                try jws.objectField("name");
                try jws.write(f.name);
                try jws.objectField("args");
                try jws.write(f.args);
            },
            .connective => |f| {
                try jws.objectField("kind");
                try jws.write(f.kind);
                try jws.objectField("operands");
                try jws.write(f.operands);
            },
            .quantifier => |f| {
                try jws.objectField("kind");
                try jws.write(f.kind);
                try jws.objectField("name");
                try jws.write(f.name);
                try jws.objectField("sort");
                try jws.write(f.sort);
                try jws.objectField("body");
                try jws.write(f.body.*);
            },
            .choice => |f| {
                try jws.objectField("kind");
                try jws.write("choice");
                try jws.objectField("varName");
                try jws.write(f.var_name);
                try jws.objectField("sort");
                try jws.write(f.sort);
                try jws.objectField("body");
                try jws.write(f.body.*);
            },
        }
        try jws.endObject();
    }
};

// Convenience constructors

pub fn Var(name: []const u8, sort: Sort) Term {
    return .{ .var_term = .{ .name = name, .sort = sort } };
}

pub fn Const(value: Term.Value, sort: Sort) Term {
    return .{ .const_term = .{ .value = value, .sort = sort } };
}

pub fn Ctor(name: []const u8, args: []const Term, sort: Sort) Term {
    return .{ .ctor_term = .{ .name = name, .args = args, .sort = sort } };
}

pub fn Lambda(param_name: []const u8, param_sort: Sort, body: *const Term, sort: Sort) Term {
    return .{ .lambda_term = .{ .param_name = param_name, .param_sort = param_sort, .body = body, .sort = sort } };
}

pub fn Let(bindings: []const Term.LetBinding, body: *const Term, sort: Sort) Term {
    return .{ .let_term = .{ .bindings = bindings, .body = body, .sort = sort } };
}

pub fn Atomic(name: []const u8, args: []const Term) Formula {
    return .{ .atomic = .{ .name = name, .args = args } };
}

pub fn And(operands: []const Formula) Formula {
    return .{ .connective = .{ .kind = .and, .operands = operands } };
}

pub fn Or(operands: []const Formula) Formula {
    return .{ .connective = .{ .kind = .or, .operands = operands } };
}

pub fn Not(operand: *const Formula) Formula {
    return .{ .connective = .{ .kind = .not, .operands = &.{operand.*} } };
}

pub fn Implies(left: *const Formula, right: *const Formula) Formula {
    return .{ .connective = .{ .kind = .implies, .operands = &.{ left.*, right.* } } };
}

pub fn Forall(name: []const u8, sort_: Sort, body: *const Formula) Formula {
    return .{ .quantifier = .{ .kind = .forall, .name = name, .sort = sort_, .body = body } };
}

pub fn Exists(name: []const u8, sort_: Sort, body: *const Formula) Formula {
    return .{ .quantifier = .{ .kind = .exists, .name = name, .sort = sort_, .body = body } };
}

pub fn Choice(var_name: []const u8, sort_: Sort, body: *const Formula) Formula {
    return .{ .choice = .{ .var_name = var_name, .sort = sort_, .body = body } };
}

// IR Document top-level

pub const IrDocument = struct {
    version: []const u8 = "provekit-ir/1.1.0",
    declarations: []const Declaration,

    pub const Declaration = union(enum) {
        property: PropertyDecl,
        bridge: BridgeDecl,
        contract: ContractDecl,

        pub const PropertyDecl = struct {
            name: []const u8,
            params: []const Param,
            body: Formula,

            pub const Param = struct {
                name: []const u8,
                sort: Sort,
            };
        };

        pub const BridgeDecl = struct {
            source_symbol: []const u8,
            source_contract_cid: []const u8,
            target_contract_cid: []const u8,
            evidence: ?[]const u8 = null,
        };

        pub const ContractDecl = struct {
            symbol: []const u8,
            precondition: ?Formula = null,
            postcondition: Formula,
            invariant: ?Formula = null,
            evidence: ?[]const u8 = null,
        };

        pub fn jsonStringify(self: Declaration, jws: anytype) !void {
            switch (self) {
                .property => |d| {
                    try jws.beginObject();
                    try jws.objectField("kind");
                    try jws.write("property");
                    try jws.objectField("name");
                    try jws.write(d.name);
                    try jws.objectField("params");
                    try jws.write(d.params);
                    try jws.objectField("body");
                    try jws.write(d.body);
                    try jws.endObject();
                },
                .bridge => |d| {
                    try jws.beginObject();
                    try jws.objectField("kind");
                    try jws.write("bridge");
                    try jws.objectField("sourceSymbol");
                    try jws.write(d.source_symbol);
                    try jws.objectField("sourceContractCid");
                    try jws.write(d.source_contract_cid);
                    try jws.objectField("targetContractCid");
                    try jws.write(d.target_contract_cid);
                    if (d.evidence) |e| {
                        try jws.objectField("evidence");
                        try jws.write(e);
                    }
                    try jws.endObject();
                },
                .contract => |d| {
                    try jws.beginObject();
                    try jws.objectField("kind");
                    try jws.write("contract");
                    try jws.objectField("symbol");
                    try jws.write(d.symbol);
                    if (d.precondition) |pre| {
                        try jws.objectField("precondition");
                        try jws.write(pre);
                    }
                    try jws.objectField("postcondition");
                    try jws.write(d.postcondition);
                    if (d.invariant) |inv| {
                        try jws.objectField("invariant");
                        try jws.write(inv);
                    }
                    if (d.evidence) |e| {
                        try jws.objectField("evidence");
                        try jws.write(e);
                    }
                    try jws.endObject();
                },
            }
        }
    };
};

// Escape HTML in JSON to match other kits
pub fn writeJson(alloc: std.mem.Allocator, value: anytype) ![]u8 {
    var list = std.ArrayList(u8).init(alloc);
    errdefer list.deinit();
    try std.json.stringify(value, .{ .emit_strings_as_arrays = false, .escape_solidus = false }, list.writer());
    return list.toOwnedSlice();
}
