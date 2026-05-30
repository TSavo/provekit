// ForwardPropagator: accumulate posts and emit implication-check diagnostics.
// Per: docs/lsp/forward-propagation-floor-v1.md
const std = @import("std");

pub const Post = struct {
    constraints: []const []const u8,
    is_top: bool,

    pub fn top() Post {
        return .{ .constraints = &.{}, .is_top = true };
    }

    pub fn of(constr: []const u8) Post {
        return .{ .constraints = &.{constr}, .is_top = false };
    }
};

pub const DiagnosticResult = struct {
    code: []const u8,
    message: []const u8,
};

pub const ForwardPropagator = struct {
    seed_catalog: std.AutoHashMap([]const u8, Post),

    pub fn init(allocator: std.mem.Allocator) ForwardPropagator {
        return .{ .seed_catalog = std.AutoHashMap([]const u8, Post).init(allocator) };
    }

    pub fn addToCatalog(self: *ForwardPropagator, callee_id: []const u8, pre: Post, post: Post) !void {
        try self.seed_catalog.put(callee_id, post);
    }

    pub fn checkCallsite(self: *ForwardPropagator, callee_id: []const u8, current_post: Post) ?DiagnosticResult {
        if (current_post.is_top) return null;
        const callee_pre = self.seed_catalog.get(callee_id) orelse return null;
        for (current_post.constraints) |c| {
            var found = false;
            for (callee_pre.constraints) |cp| {
                if (std.mem.eql(u8, c, cp)) {
                    found = true;
                    break;
                }
            }
            if (!found) {
                return .{
                    .code = "provekit.lsp.implication_failed",
                    .message = "post does not imply callee pre",
                };
            }
        }
        return null;
    }
};
