/* SPDX-License-Identifier: Apache-2.0 */
/*
 * lift-plugin protocol RPC handler. Speaks pep/1.7.0 over NDJSON
 * on stdio. Persistent daemon: stays up across multiple lift calls,
 * exits on `shutdown` or stdin EOF.
 *
 * Mirrors implementations/java/.../Rpc.java (the freshest cleanest peer)
 * and implementations/typescript/.../mint-ts-self-contracts-rpc.cjs (the
 * daemon-lifecycle pattern from PR #220).
 *
 * Handshake:
 *   -> initialize
 *   <- {name, version, protocol_version, capabilities}
 *   -> lift
 *   <- {kind:"proof-envelope", filename_cid, contract_set_cid, bytes_base64,
 *       diagnostics:[]}
 *   -> shutdown
 *   <- null   (then process exits)
 *
 * Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
 *
 * JSON parsing is hand-rolled. The request shape is small and fixed
 * (only `id` and `method` are read). Responses are emitted as strict
 * JSON via a tiny writer that escapes the same characters JCS does.
 * All emitted strings are CIDs / catalog literals / static names so
 * non-ASCII never appears.
 */

/*
 * _GNU_SOURCE: getline() is a POSIX 2008 / GNU extension, gated behind
 * _GNU_SOURCE on glibc. Without this define, getline() declaration is
 * suppressed and the build errors with "implicit declaration".
 */
#define _GNU_SOURCE

#include "rpc.h"
#include "orchestrator.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>  /* ssize_t */

/* ----------------------------------------------------------------------- */
/* Base64 (stdpad) — for bytes_base64 in lift response                      */
/* ----------------------------------------------------------------------- */

static const char B64_TABLE[] =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static char *base64_encode(const unsigned char *data, size_t len) {
    size_t out_len = ((len + 2) / 3) * 4;
    char *out = (char *)malloc(out_len + 1);
    if (!out) return NULL;
    size_t o = 0;
    size_t i = 0;
    while (i + 3 <= len) {
        unsigned int n = ((unsigned)data[i] << 16) | ((unsigned)data[i+1] << 8) | data[i+2];
        out[o++] = B64_TABLE[(n >> 18) & 0x3F];
        out[o++] = B64_TABLE[(n >> 12) & 0x3F];
        out[o++] = B64_TABLE[(n >> 6) & 0x3F];
        out[o++] = B64_TABLE[n & 0x3F];
        i += 3;
    }
    size_t rem = len - i;
    if (rem == 1) {
        unsigned int n = ((unsigned)data[i] << 16);
        out[o++] = B64_TABLE[(n >> 18) & 0x3F];
        out[o++] = B64_TABLE[(n >> 12) & 0x3F];
        out[o++] = '=';
        out[o++] = '=';
    } else if (rem == 2) {
        unsigned int n = ((unsigned)data[i] << 16) | ((unsigned)data[i+1] << 8);
        out[o++] = B64_TABLE[(n >> 18) & 0x3F];
        out[o++] = B64_TABLE[(n >> 12) & 0x3F];
        out[o++] = B64_TABLE[(n >> 6) & 0x3F];
        out[o++] = '=';
    }
    out[o] = '\0';
    return out;
}

/* ----------------------------------------------------------------------- */
/* tiny NDJSON request parsers                                              */
/* ----------------------------------------------------------------------- */

/* Find `"key"` in `s`, then return a pointer to the first non-whitespace
 * character after the following `:`. Returns NULL if not found. Caller
 * does NOT own the returned pointer (it's into `s`). */
static const char *find_field(const char *s, const char *key) {
    size_t klen = strlen(key);
    /* Search for "<key>" */
    const char *p = s;
    while ((p = strchr(p, '"')) != NULL) {
        if (strncmp(p + 1, key, klen) == 0 && p[1 + klen] == '"') {
            const char *q = p + 1 + klen + 1;
            while (*q && isspace((unsigned char)*q)) q++;
            if (*q != ':') { p++; continue; }
            q++;
            while (*q && isspace((unsigned char)*q)) q++;
            return q;
        }
        p++;
    }
    return NULL;
}

/* Parse a JSON string value at `p` into a malloc'd C string (no escape
 * decoding beyond removing the surrounding quotes — sufficient for the
 * `method` field we read). Returns NULL on failure. */
static char *parse_string(const char *p) {
    if (!p || *p != '"') return NULL;
    p++;
    const char *start = p;
    while (*p && *p != '"') {
        if (*p == '\\' && p[1]) p += 2;
        else p++;
    }
    if (*p != '"') return NULL;
    size_t len = (size_t)(p - start);
    char *out = (char *)malloc(len + 1);
    if (!out) return NULL;
    memcpy(out, start, len);
    out[len] = '\0';
    return out;
}

/* Capture the literal id token (number, "string", true, false, null) at
 * `p` into a malloc'd C string. We pass it through verbatim into the
 * response. Returns NULL on failure (treat as id=null in caller). */
static char *capture_id_literal(const char *p) {
    if (!p) return NULL;
    if (*p == '"') {
        const char *q = p + 1;
        while (*q && *q != '"') {
            if (*q == '\\' && q[1]) q += 2; else q++;
        }
        if (*q != '"') return NULL;
        size_t len = (size_t)(q - p + 1);
        char *out = (char *)malloc(len + 1);
        if (!out) return NULL;
        memcpy(out, p, len);
        out[len] = '\0';
        return out;
    }
    /* number / true / false / null: read until terminator. */
    const char *q = p;
    while (*q && *q != ',' && *q != '}' && !isspace((unsigned char)*q)) q++;
    size_t len = (size_t)(q - p);
    if (len == 0) return NULL;
    char *out = (char *)malloc(len + 1);
    if (!out) return NULL;
    memcpy(out, p, len);
    out[len] = '\0';
    return out;
}

/* ----------------------------------------------------------------------- */
/* response writers                                                         */
/* ----------------------------------------------------------------------- */

/* Escape `s` as a JSON string literal (no surrounding quotes). All inputs
 * here are ASCII-safe CIDs / catalog literals; we still escape `"` and
 * `\` and control bytes for safety. */
static char *json_escape(const char *s) {
    if (!s) return NULL;
    size_t n = strlen(s);
    /* worst case: 6x growth (\u00XX) */
    char *out = (char *)malloc(n * 6 + 1);
    if (!out) return NULL;
    size_t o = 0;
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        if (c == '"') { out[o++] = '\\'; out[o++] = '"'; }
        else if (c == '\\') { out[o++] = '\\'; out[o++] = '\\'; }
        else if (c < 0x20) {
            out[o++] = '\\';
            out[o++] = 'u';
            out[o++] = '0';
            out[o++] = '0';
            const char *hex = "0123456789abcdef";
            out[o++] = hex[(c >> 4) & 0xF];
            out[o++] = hex[c & 0xF];
        } else {
            out[o++] = (char)c;
        }
    }
    out[o] = '\0';
    return out;
}

static const char *id_or_null(const char *id) {
    return id ? id : "null";
}

static void write_initialize_response(const char *id) {
    fprintf(stdout,
        "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{"
        "\"name\":\"c-self-contracts\","
        "\"version\":\"1.0.0\","
        "\"protocol_version\":\"pep/1.7.0\","
        "\"capabilities\":{"
        "\"authoring_surfaces\":[\"c-self-contracts\"],"
        "\"ir_version\":\"v1.1.0\","
        "\"emits_signed_mementos\":true"
        "}}}\n",
        id_or_null(id));
    fflush(stdout);
}

static void write_error(const char *id, int code, const char *message) {
    char *esc = json_escape(message);
    fprintf(stdout,
        "{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":{\"code\":%d,\"message\":\"%s\"}}\n",
        id_or_null(id), code, esc ? esc : "");
    fflush(stdout);
    free(esc);
}

static void write_shutdown_response(const char *id) {
    fprintf(stdout, "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":null}\n",
            id_or_null(id));
    fflush(stdout);
}

static void handle_lift(const char *id) {
    mcsc_mint_result r;
    mcsc_mint_result_init(&r);
    if (mcsc_mint_one_run(NULL, &r) != 0) {
        write_error(id, 1005, "LIFT_FAILED: mint_one_run returned non-zero");
        return;
    }
    char *cid_esc = json_escape(r.cid);
    char *cset_esc = json_escape(r.contract_set_cid);
    char *b64 = base64_encode(r.bytes, r.bytes_len);
    if (!cid_esc || !cset_esc || !b64) {
        free(cid_esc); free(cset_esc); free(b64);
        mcsc_mint_result_free(&r);
        write_error(id, 1006, "LIFT_FAILED: out-of-memory encoding response");
        return;
    }
    fprintf(stdout,
        "{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{"
        "\"kind\":\"proof-envelope\","
        "\"filename_cid\":\"%s\","
        "\"contract_set_cid\":\"%s\","
        "\"bytes_base64\":\"%s\","
        "\"diagnostics\":[]"
        "}}\n",
        id_or_null(id), cid_esc, cset_esc, b64);
    fflush(stdout);
    free(cid_esc);
    free(cset_esc);
    free(b64);
    mcsc_mint_result_free(&r);
}

/* ----------------------------------------------------------------------- */
/* Public entry                                                             */
/* ----------------------------------------------------------------------- */

int mcsc_run_rpc(void) {
    char *line = NULL;
    size_t cap = 0;
#if defined(__APPLE__) || defined(__linux__)
    ssize_t n;
    while ((n = getline(&line, &cap, stdin)) != -1) {
#else
    /* Fallback: not used on POSIX systems we target. */
    (void)cap;
    while (fgets(line, 65536, stdin)) {
        size_t n = strlen(line);
#endif
        if (n == 0) continue;
        /* Trim trailing newline / whitespace. */
        while (n > 0 && (line[n-1] == '\n' || line[n-1] == '\r' || isspace((unsigned char)line[n-1]))) {
            line[--n] = '\0';
        }
        /* Skip leading whitespace. */
        char *p = line;
        while (*p && isspace((unsigned char)*p)) p++;
        if (*p == '\0') continue;

        const char *id_pos = find_field(p, "id");
        char *id_lit = id_pos ? capture_id_literal(id_pos) : NULL;

        const char *method_pos = find_field(p, "method");
        char *method = method_pos ? parse_string(method_pos) : NULL;

        if (!method) {
            write_error(id_lit, -32600, "Invalid Request: missing `method`");
            free(id_lit);
            continue;
        }

        if (strcmp(method, "initialize") == 0) {
            write_initialize_response(id_lit);
        } else if (strcmp(method, "lift") == 0) {
            handle_lift(id_lit);
        } else if (strcmp(method, "shutdown") == 0 || strcmp(method, "exit") == 0) {
            write_shutdown_response(id_lit);
            free(id_lit);
            free(method);
            free(line);
            return 0;
        } else {
            char buf[128];
            snprintf(buf, sizeof(buf), "METHOD_NOT_FOUND: %s", method);
            write_error(id_lit, -32601, buf);
        }
        free(id_lit);
        free(method);
    }
    free(line);
    return 0;
}
