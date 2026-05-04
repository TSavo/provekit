/* SPDX-License-Identifier: Apache-2.0 */
/*
 * provekit-lsp-c — NDJSON LSP plugin for C.
 *
 * Protocol (provekit-lsp-plugin/1 over stdio):
 *
 *   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
 *   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
 *   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
 *
 * For parse: scans the source for top-level C function declarations using
 * POSIX ERE (regex.h). Lifts to canonical parse result:
 *   {declarations: [...], callEdges: [...], warnings: [...]}
 *
 * Wire shape matches implementations/go/cmd/provekit-lsp-go/main.go.
 *
 * v0: regex-based parser. libclang AST is a follow-up.
 *
 * Build:
 *   cc -std=c11 -Wall -Wextra -o provekit-lsp-c main.c
 */

/* `getline` and `ssize_t` are POSIX extensions; glibc gates them behind
 * feature-test macros. Define _GNU_SOURCE before any system header is
 * pulled in (review feedback: PR #165 / CodeRabbit).
 *
 * Also include <sys/types.h> explicitly for `ssize_t` so the build doesn't
 * rely on transitive inclusion through <stdio.h>. */
#define _GNU_SOURCE
#include <sys/types.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <regex.h>

/* -----------------------------------------------------------------------
 * Dynamic string buffer
 * ----------------------------------------------------------------------- */

typedef struct {
    char   *data;
    size_t  len;
    size_t  cap;
} Buf;

static void buf_init(Buf *b) {
    b->cap  = 256;
    b->len  = 0;
    b->data = (char *)malloc(b->cap);
    if (b->data) b->data[0] = '\0';
}

static void buf_free(Buf *b) {
    free(b->data);
    b->data = NULL;
    b->len  = 0;
    b->cap  = 0;
}

static void buf_grow(Buf *b, size_t need) {
    if (b->len + need + 1 <= b->cap) return;
    size_t nc = b->cap * 2;
    while (nc < b->len + need + 1) nc *= 2;
    char *nd = (char *)realloc(b->data, nc);
    if (!nd) return;
    b->data = nd;
    b->cap  = nc;
}

static void buf_append(Buf *b, const char *s) {
    if (!s) return;
    size_t n = strlen(s);
    buf_grow(b, n);
    memcpy(b->data + b->len, s, n + 1);
    b->len += n;
}

static void buf_append_char(Buf *b, char c) {
    buf_grow(b, 1);
    b->data[b->len] = c;
    b->data[b->len + 1] = '\0';
    b->len++;
}

/* -----------------------------------------------------------------------
 * JSON helpers (hand-rolled; messages are small)
 * ----------------------------------------------------------------------- */

/* JCS-compliant string escaping per RFC 8785. */
static void json_escape_str(Buf *out, const char *s) {
    buf_append_char(out, '"');
    for (const char *p = s; *p; p++) {
        unsigned char c = (unsigned char)*p;
        if (c == '"') {
            buf_append(out, "\\\"");
        } else if (c == '\\') {
            buf_append(out, "\\\\");
        } else if (c == '\b') {
            buf_append(out, "\\b");
        } else if (c == '\f') {
            buf_append(out, "\\f");
        } else if (c == '\n') {
            buf_append(out, "\\n");
        } else if (c == '\r') {
            buf_append(out, "\\r");
        } else if (c == '\t') {
            buf_append(out, "\\t");
        } else if (c < 0x20) {
            char esc[7];
            snprintf(esc, sizeof(esc), "\\u00%02x", c);
            buf_append(out, esc);
        } else {
            buf_append_char(out, *p);
        }
    }
    buf_append_char(out, '"');
}

/* Extract the string value of the named field in a flat JSON object line.
 * field must be the exact key string (no quotes).
 * Returns a malloc'd string or NULL. */
static char *json_extract_str(const char *json, const char *field) {
    char needle[256];
    snprintf(needle, sizeof(needle), "\"%s\"", field);

    const char *p = strstr(json, needle);
    if (!p) return NULL;

    p += strlen(needle);
    /* skip : and whitespace */
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (*p != '"') return NULL;
    p++; /* skip opening quote */

    Buf result;
    buf_init(&result);

    while (*p && *p != '"') {
        if (*p == '\\' && *(p + 1)) {
            p++;
            switch (*p) {
                case '"':  buf_append_char(&result, '"');  break;
                case '\\': buf_append_char(&result, '\\'); break;
                case 'b':  buf_append_char(&result, '\b'); break;
                case 'f':  buf_append_char(&result, '\f'); break;
                case 'n':  buf_append_char(&result, '\n'); break;
                case 'r':  buf_append_char(&result, '\r'); break;
                case 't':  buf_append_char(&result, '\t'); break;
                case 'u': {
                    /* Minimal 4-hex decode for BMP range */
                    if (*(p+1) && *(p+2) && *(p+3) && *(p+4)) {
                        unsigned int cp = 0;
                        sscanf(p + 1, "%4x", &cp);
                        p += 4;
                        if (cp < 0x80) {
                            buf_append_char(&result, (char)cp);
                        } else if (cp < 0x800) {
                            buf_append_char(&result, (char)(0xC0 | (cp >> 6)));
                            buf_append_char(&result, (char)(0x80 | (cp & 0x3F)));
                        } else {
                            buf_append_char(&result, (char)(0xE0 | (cp >> 12)));
                            buf_append_char(&result, (char)(0x80 | ((cp >> 6) & 0x3F)));
                            buf_append_char(&result, (char)(0x80 | (cp & 0x3F)));
                        }
                    }
                    break;
                }
                default: buf_append_char(&result, *p); break;
            }
        } else {
            buf_append_char(&result, *p);
        }
        p++;
    }

    char *s = result.data;
    result.data = NULL;
    buf_free(&result);
    return s;
}

/* Extract integer id from JSON line (may be string or number). */
static char *json_extract_id(const char *json) {
    const char *p = strstr(json, "\"id\"");
    if (!p) return NULL;
    p += 4;
    while (*p == ':' || *p == ' ' || *p == '\t') p++;
    if (!*p || *p == '}') return NULL;

    Buf b;
    buf_init(&b);

    /* Collect up to next ',' or '}', trimming whitespace */
    while (*p && *p != ',' && *p != '}') {
        buf_append_char(&b, *p);
        p++;
    }
    /* Trim trailing whitespace */
    while (b.len > 0 && (b.data[b.len - 1] == ' ' || b.data[b.len - 1] == '\t'))
        b.data[--b.len] = '\0';

    char *s = b.data;
    b.data = NULL;
    buf_free(&b);
    return s;
}

/* Extract "method" field value (unquoted result string). */
static char *json_extract_method(const char *json) {
    return json_extract_str(json, "method");
}

/* -----------------------------------------------------------------------
 * C source parser — POSIX ERE regex lifter
 *
 * Extracts:
 *   declarations — top-level function definitions:
 *     <type> name(params) {   or
 *     <type> name(params);   (forward decl)
 *
 *   callEdges — call sites: name(  inside function bodies
 * ----------------------------------------------------------------------- */

#define MAX_DECLS 256
#define MAX_CALLS 1024
#define MAX_NAME  256
#define MAX_LINES 65536

typedef struct {
    char name[MAX_NAME];
    int  line;
} Decl;

typedef struct {
    char caller[MAX_NAME];
    char callee[MAX_NAME];
    int  line;
} CallEdge;

typedef struct {
    Decl     decls[MAX_DECLS];
    int      n_decls;
    CallEdge edges[MAX_CALLS];
    int      n_edges;
    char     warnings[4096];
} ParseResult;

/*
 * POSIX ERE patterns.
 *
 * Function definition:
 *   Matches lines of the form:
 *     <return-type> <name> (
 *   where return-type is one or more C type tokens (possibly *, const, etc.)
 *   and name is an identifier.
 *
 * Pattern:  ^[[:space:]]*[A-Za-z_][A-Za-z0-9_ *]*[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\(
 *
 * Call site:
 *   Matches:  ([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\(
 *   (any identifier followed by a left paren — heuristic)
 */

static regex_t re_funcdef;
static regex_t re_callsite;

static int regexes_compiled = 0;

static int compile_regexes(void) {
    if (regexes_compiled) return 0;

    int r;
    r = regcomp(&re_funcdef,
        "^[[:space:]]*[A-Za-z_][A-Za-z0-9_ *]*[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\\(",
        REG_EXTENDED);
    if (r != 0) {
        char errbuf[256];
        regerror(r, &re_funcdef, errbuf, sizeof(errbuf));
        fprintf(stderr, "provekit-lsp-c: regex compile funcdef: %s\n", errbuf);
        return -1;
    }

    r = regcomp(&re_callsite,
        "([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*\\(",
        REG_EXTENDED);
    if (r != 0) {
        char errbuf[256];
        regerror(r, &re_callsite, errbuf, sizeof(errbuf));
        fprintf(stderr, "provekit-lsp-c: regex compile callsite: %s\n", errbuf);
        regfree(&re_funcdef);
        return -1;
    }

    regexes_compiled = 1;
    return 0;
}

/* C keywords that look like function calls but aren't. */
static int is_keyword(const char *name) {
    static const char *kw[] = {
        "if", "else", "for", "while", "do", "switch", "case", "return",
        "break", "continue", "goto", "sizeof", "typeof", "alignof",
        "static", "extern", "const", "volatile", "inline", "register",
        "void", "int", "char", "short", "long", "float", "double",
        "unsigned", "signed", "struct", "union", "enum", "typedef",
        NULL
    };
    for (int i = 0; kw[i]; i++) {
        if (strcmp(name, kw[i]) == 0) return 1;
    }
    return 0;
}

/*
 * Check whether a line (already NUL-terminated, leading whitespace stripped)
 * starts with //provekit:contract  (no space between // and provekit).
 * Matches the C++ scanner convention in implementations/cpp/provekit-lsp-cpp.
 */
static int is_contract_annotation(const char *line) {
    /* Skip leading whitespace. */
    while (*line == ' ' || *line == '\t') line++;
    return strncmp(line, "//provekit:contract", 19) == 0;
}

static ParseResult parse_c_source(const char *source) {
    ParseResult result;
    memset(&result, 0, sizeof(result));

    if (compile_regexes() != 0) {
        snprintf(result.warnings, sizeof(result.warnings),
                 "regex compile failed; no declarations extracted");
        return result;
    }

    /* Split source into lines. */
    const char *lines_start[MAX_LINES];
    int         lines_len[MAX_LINES];
    int         n_lines = 0;

    const char *p = source;
    while (*p && n_lines < MAX_LINES) {
        lines_start[n_lines] = p;
        const char *eol = strchr(p, '\n');
        if (!eol) {
            lines_len[n_lines] = (int)strlen(p);
            n_lines++;
            break;
        }
        lines_len[n_lines] = (int)(eol - p);
        n_lines++;
        p = eol + 1;
    }

    /* Track the "current function" for call-edge attribution. */
    char current_fn[MAX_NAME] = "";
    int  brace_depth = 0;

    /*
     * annotate_next: set to 1 when we see a //provekit:contract line.
     * The next function definition is emitted as a kind:"contract" declaration.
     */
    int annotate_next = 0;

    for (int i = 0; i < n_lines; i++) {
        /* Copy line to a NUL-terminated buffer. */
        int llen = lines_len[i];
        if (llen < 0) llen = 0;
        char line[4096];
        if (llen >= (int)sizeof(line)) llen = (int)sizeof(line) - 1;
        memcpy(line, lines_start[i], (size_t)llen);
        line[llen] = '\0';

        /* Check for annotation comment. */
        if (is_contract_annotation(line)) {
            annotate_next = 1;
            continue;
        }

        /* Track brace depth for current function scope. */
        for (int ci = 0; line[ci]; ci++) {
            if (line[ci] == '{') {
                brace_depth++;
            } else if (line[ci] == '}') {
                if (brace_depth > 0) brace_depth--;
                if (brace_depth == 0) current_fn[0] = '\0';
            }
        }

        /* Try to match function definition at this line. */
        regmatch_t m[3];
        if (regexec(&re_funcdef, line, 3, m, 0) == 0 && m[1].rm_so >= 0) {
            int nlen = (int)(m[1].rm_eo - m[1].rm_so);
            if (nlen >= MAX_NAME) nlen = MAX_NAME - 1;
            char fname[MAX_NAME];
            memcpy(fname, line + m[1].rm_so, (size_t)nlen);
            fname[nlen] = '\0';

            if (!is_keyword(fname)) {
                /* Emit as contract declaration only if annotated. */
                if (annotate_next && result.n_decls < MAX_DECLS) {
                    snprintf(result.decls[result.n_decls].name, MAX_NAME, "%s", fname);
                    result.decls[result.n_decls].line = i + 1;
                    result.n_decls++;
                }
                annotate_next = 0;

                /* If this line opens a body, set current_fn for call-edge tracking.
                 * Also handle Allman-style functions where the opening brace is
                 * on the next non-blank line (review feedback: PR #165 / CodeRabbit). */
                int opens_body = (strchr(line, '{') != NULL);
                if (!opens_body) {
                    /* Peek forward for an Allman-style brace on the next non-blank line. */
                    for (int j = i + 1; j < n_lines && j < i + 4; j++) {
                        int jlen = lines_len[j];
                        if (jlen < 0) jlen = 0;
                        if (jlen >= (int)sizeof(line)) jlen = (int)sizeof(line) - 1;
                        char nextline[4096];
                        memcpy(nextline, lines_start[j], (size_t)jlen);
                        nextline[jlen] = '\0';
                        /* Skip blank lines. */
                        int blank = 1;
                        for (int ci = 0; nextline[ci]; ci++) {
                            if (nextline[ci] != ' ' && nextline[ci] != '\t' &&
                                nextline[ci] != '\r' && nextline[ci] != '\n') {
                                blank = 0;
                                break;
                            }
                        }
                        if (blank) continue;
                        /* First non-blank: a leading '{' (after whitespace) means Allman. */
                        for (int ci = 0; nextline[ci]; ci++) {
                            if (nextline[ci] == ' ' || nextline[ci] == '\t') continue;
                            if (nextline[ci] == '{') opens_body = 1;
                            break;
                        }
                        break;
                    }
                }
                if (opens_body) {
                    snprintf(current_fn, MAX_NAME, "%s", fname);
                }
            }
        } else {
            /* Non-function line resets annotation flag only if it's not blank. */
            if (annotate_next) {
                /* Keep annotate_next alive across blank lines between annotation
                 * and function definition. Reset only on non-blank non-fn lines
                 * that are not themselves annotations. */
                int blank = 1;
                for (int ci = 0; line[ci]; ci++) {
                    if (line[ci] != ' ' && line[ci] != '\t' &&
                        line[ci] != '\r' && line[ci] != '\n') {
                        blank = 0;
                        break;
                    }
                }
                /* Blank lines: keep flag. Non-blank non-fn lines (e.g. comments,
                 * preprocessor): reset. */
                if (!blank) {
                    annotate_next = 0;
                }
            }
        }

        /* Scan for call sites within a known function body. */
        if (current_fn[0] != '\0') {
            const char *scan = line;
            regmatch_t cm[2];
            while (regexec(&re_callsite, scan, 2, cm, 0) == 0 && cm[1].rm_so >= 0) {
                int clen = (int)(cm[1].rm_eo - cm[1].rm_so);
                if (clen < MAX_NAME) {
                    char callee[MAX_NAME];
                    memcpy(callee, scan + cm[1].rm_so, (size_t)clen);
                    callee[clen] = '\0';

                    if (!is_keyword(callee) &&
                        strcmp(callee, current_fn) != 0 &&
                        result.n_edges < MAX_CALLS)
                    {
                        snprintf(result.edges[result.n_edges].caller, MAX_NAME, "%s", current_fn);
                        snprintf(result.edges[result.n_edges].callee, MAX_NAME, "%s", callee);
                        result.edges[result.n_edges].line = i + 1;
                        result.n_edges++;
                    }
                }
                scan += cm[1].rm_eo;
                if (*scan == '\0') break;
            }
        }
    }

    return result;
}

/* -----------------------------------------------------------------------
 * Response writers
 * ----------------------------------------------------------------------- */

static void send_response(const char *id, const char *result_json) {
    /* Output: {"jsonrpc":"2.0","id":<id>,"result":<result>} */
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":%s}\n", id, result_json);
    fflush(stdout);
}

static void send_error(const char *id, int code, const char *message) {
    Buf b;
    buf_init(&b);
    buf_append(&b, "{\"code\":");
    char cstr[32];
    snprintf(cstr, sizeof(cstr), "%d", code);
    buf_append(&b, cstr);
    buf_append(&b, ",\"message\":");
    json_escape_str(&b, message);
    buf_append(&b, "}");

    const char *safe_id = (id && *id) ? id : "null";
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":%s}\n", safe_id, b.data);
    fflush(stdout);

    buf_free(&b);
}

static void handle_initialize(const char *id) {
    /* Per wire shape: {name, version, capabilities:["parse"]} */
    send_response(id,
        "{\"capabilities\":[\"parse\"],"
        "\"name\":\"provekit-lsp-c\","
        "\"version\":\"0.1.0\"}");
}

static void handle_parse(const char *id, const char *json_line) {
    /* Extract path and source from params. */
    char *source = json_extract_str(json_line, "source");
    /* path is informational only; we don't need it for regex parsing */

    if (!source) {
        send_error(id, -32602, "parse: missing params.source");
        return;
    }

    ParseResult pr = parse_c_source(source);
    free(source);

    /* Build declarations JSON array.
     * Only //provekit:contract annotated functions are emitted.
     * Shape: {"kind":"contract","name":"<fn>","outBinding":"out"}
     * Keys in JCS order: kind < name < outBinding (by Unicode code point). */
    Buf decls_buf;
    buf_init(&decls_buf);
    buf_append_char(&decls_buf, '[');
    for (int i = 0; i < pr.n_decls; i++) {
        if (i > 0) buf_append_char(&decls_buf, ',');
        buf_append(&decls_buf, "{\"kind\":\"contract\",\"name\":");
        json_escape_str(&decls_buf, pr.decls[i].name);
        buf_append(&decls_buf, ",\"outBinding\":\"out\"}");
    }
    buf_append_char(&decls_buf, ']');

    /* Build callEdges JSON array.
     *
     * The canonical IR shape is {sourceContractCid, targetContractCid,
     * targetSymbol, callSiteLocus, evidenceTerm} per spec #114. The C LSP
     * cannot compute contract CIDs (no JCS encoder + BLAKE3 here), so the
     * legacy {callee, caller, line} shape was silently dropped by the daemon.
     * Emit an empty array until contract-CID computation is available; this
     * is graceful downgrade rather than emitting a non-canonical shape.
     * (Review feedback: PR #165 / Copilot.) */
    Buf edges_buf;
    buf_init(&edges_buf);
    buf_append(&edges_buf, "[]");
    /* Suppress unused-variable warning: we still parse call sites for
     * future shape upgrade; pr.n_edges is intentionally unread here. */
    (void)pr.n_edges;

    /* Build warnings array. */
    Buf warn_buf;
    buf_init(&warn_buf);
    if (pr.warnings[0]) {
        buf_append(&warn_buf, "[");
        json_escape_str(&warn_buf, pr.warnings);
        buf_append(&warn_buf, "]");
    } else {
        buf_append(&warn_buf, "[]");
    }

    /* Assemble result. JCS sorted keys: callEdges < declarations < warnings */
    Buf result;
    buf_init(&result);
    buf_append(&result, "{\"callEdges\":");
    buf_append(&result, edges_buf.data);
    buf_append(&result, ",\"declarations\":");
    buf_append(&result, decls_buf.data);
    buf_append(&result, ",\"warnings\":");
    buf_append(&result, warn_buf.data);
    buf_append_char(&result, '}');

    send_response(id, result.data);

    buf_free(&decls_buf);
    buf_free(&edges_buf);
    buf_free(&warn_buf);
    buf_free(&result);
}

static void handle_shutdown(const char *id) {
    send_response(id, "null");
}

/* -----------------------------------------------------------------------
 * Main NDJSON dispatcher
 * ----------------------------------------------------------------------- */

int main(int argc, char **argv) {
    int rpc_mode = 0;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--rpc") == 0) {
            rpc_mode = 1;
        }
    }

    if (!rpc_mode) {
        fprintf(stderr, "Usage: provekit-lsp-c --rpc\n");
        fprintf(stderr, "  Speaks provekit-lsp-plugin/1 NDJSON over stdin/stdout.\n");
        return 1;
    }

    char *line = NULL;
    size_t line_cap = 0;
    ssize_t line_len;

    while ((line_len = getline(&line, &line_cap, stdin)) != -1) {
        /* Strip trailing newline. */
        while (line_len > 0 && (line[line_len - 1] == '\n' || line[line_len - 1] == '\r')) {
            line[--line_len] = '\0';
        }
        if (line_len == 0) continue;

        char *method = json_extract_method(line);
        char *id     = json_extract_id(line);
        const char *safe_id = (id && *id) ? id : "null";

        if (!method) {
            send_error(safe_id, -32700, "parse error: could not extract method");
        } else if (strcmp(method, "initialize") == 0) {
            handle_initialize(safe_id);
        } else if (strcmp(method, "parse") == 0) {
            handle_parse(safe_id, line);
        } else if (strcmp(method, "shutdown") == 0) {
            handle_shutdown(safe_id);
            free(method);
            free(id);
            break;
        } else if (strcmp(method, "exit") == 0) {
            free(method);
            free(id);
            break;
        } else {
            Buf msg;
            buf_init(&msg);
            buf_append(&msg, "unknown method: ");
            buf_append(&msg, method);
            send_error(safe_id, -32601, msg.data);
            buf_free(&msg);
        }

        free(method);
        free(id);
    }

    free(line);
    if (regexes_compiled) {
        regfree(&re_funcdef);
        regfree(&re_callsite);
    }
    return 0;
}
