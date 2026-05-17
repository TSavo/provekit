/* SPDX-License-Identifier: Apache-2.0 */

#include <ctype.h>
#include <errno.h>
#include <stdint.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "blake3.h"

#define BODY_TEMPLATE_REL "menagerie/c-language-signature/specs/body-templates/c-canonical-bodies.json"
#define CONCEPT_CITATION_COMMENT_KIND "provekit-concept-citation-comment-sugar"
#define DEFAULT_KIT_ID "provekit-realize-c-core@0.1.0"
#define DEFAULT_TARGET_LIBRARY_TAG "c-core"
#define MAX_LINE 131072

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buf;

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} StringArray;

typedef struct {
    char *concept_name;
    char *template_kind;
    char *template_text;
    int has_min_params;
    int min_params;
    int has_max_params;
    int max_params;
    StringArray requires_param_types;
    int has_requires_param_types;
    char *requires_return_type;
} BodyTemplateEntry;

typedef struct {
    BodyTemplateEntry *entries;
    size_t len;
    size_t cap;
} TemplateCatalog;

static char *xstrdup(const char *s) {
    size_t n = strlen(s);
    char *out = (char *)malloc(n + 1);
    if (out == NULL) return NULL;
    memcpy(out, s, n + 1);
    return out;
}

static char *xstrndup(const char *s, size_t n) {
    char *out = (char *)malloc(n + 1);
    if (out == NULL) return NULL;
    memcpy(out, s, n);
    out[n] = '\0';
    return out;
}

static void buf_init(Buf *b) {
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static void buf_free(Buf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static int buf_reserve(Buf *b, size_t extra) {
    size_t need = b->len + extra + 1;
    if (need <= b->cap) return 0;
    size_t next = b->cap == 0 ? 256 : b->cap;
    while (next < need) {
        if (next > (SIZE_MAX / 2)) return -1;
        next *= 2;
    }
    char *data = (char *)realloc(b->data, next);
    if (data == NULL) return -1;
    b->data = data;
    b->cap = next;
    if (b->len == 0) b->data[0] = '\0';
    return 0;
}

static int buf_append_n(Buf *b, const char *s, size_t n) {
    if (buf_reserve(b, n) != 0) return -1;
    memcpy(b->data + b->len, s, n);
    b->len += n;
    b->data[b->len] = '\0';
    return 0;
}

static int buf_append(Buf *b, const char *s) {
    return buf_append_n(b, s, strlen(s));
}

static int buf_append_char(Buf *b, char c) {
    return buf_append_n(b, &c, 1);
}

static char *buf_steal(Buf *b) {
    if (b->data == NULL) {
        return xstrdup("");
    }
    char *out = b->data;
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
    return out;
}

static void string_array_init(StringArray *arr) {
    arr->items = NULL;
    arr->len = 0;
    arr->cap = 0;
}

static int string_array_push(StringArray *arr, char *item) {
    if (arr->len + 1 > arr->cap) {
        size_t next = arr->cap == 0 ? 4 : arr->cap * 2;
        char **items = (char **)realloc(arr->items, next * sizeof(char *));
        if (items == NULL) return -1;
        arr->items = items;
        arr->cap = next;
    }
    arr->items[arr->len++] = item;
    return 0;
}

static void string_array_free(StringArray *arr) {
    for (size_t i = 0; i < arr->len; i++) {
        free(arr->items[i]);
    }
    free(arr->items);
    arr->items = NULL;
    arr->len = 0;
    arr->cap = 0;
}

static void catalog_init(TemplateCatalog *catalog) {
    catalog->entries = NULL;
    catalog->len = 0;
    catalog->cap = 0;
}

static void entry_free(BodyTemplateEntry *entry) {
    free(entry->concept_name);
    free(entry->template_kind);
    free(entry->template_text);
    string_array_free(&entry->requires_param_types);
    free(entry->requires_return_type);
}

static void catalog_free(TemplateCatalog *catalog) {
    for (size_t i = 0; i < catalog->len; i++) {
        entry_free(&catalog->entries[i]);
    }
    free(catalog->entries);
    catalog->entries = NULL;
    catalog->len = 0;
    catalog->cap = 0;
}

static int catalog_push(TemplateCatalog *catalog, BodyTemplateEntry entry) {
    if (catalog->len + 1 > catalog->cap) {
        size_t next = catalog->cap == 0 ? 16 : catalog->cap * 2;
        BodyTemplateEntry *entries =
            (BodyTemplateEntry *)realloc(catalog->entries, next * sizeof(BodyTemplateEntry));
        if (entries == NULL) return -1;
        catalog->entries = entries;
        catalog->cap = next;
    }
    catalog->entries[catalog->len++] = entry;
    return 0;
}

static const char *skip_ws(const char *p, const char *end) {
    while (p < end && isspace((unsigned char)*p)) p++;
    return p;
}

static const char *skip_string_raw(const char *p, const char *end) {
    if (p >= end || *p != '"') return NULL;
    p++;
    while (p < end) {
        if (*p == '\\') {
            p++;
            if (p < end) p++;
            continue;
        }
        if (*p == '"') return p + 1;
        p++;
    }
    return NULL;
}

static const char *json_value_end(const char *p, const char *end) {
    p = skip_ws(p, end);
    if (p >= end) return NULL;
    if (*p == '"') return skip_string_raw(p, end);
    if (*p == '{' || *p == '[') {
        char open = *p;
        char close = open == '{' ? '}' : ']';
        int depth = 0;
        while (p < end) {
            if (*p == '"') {
                p = skip_string_raw(p, end);
                if (p == NULL) return NULL;
                continue;
            }
            if (*p == open) depth++;
            if (*p == close) {
                depth--;
                if (depth == 0) return p + 1;
            }
            p++;
        }
        return NULL;
    }
    while (p < end && *p != ',' && *p != '}' && *p != ']' &&
           !isspace((unsigned char)*p)) {
        p++;
    }
    return p;
}

static int raw_string_key_matches(const char *p, const char *end, const char *key,
                                  const char **after_quote) {
    if (p >= end || *p != '"') return 0;
    p++;
    const char *q = p;
    size_t klen = strlen(key);
    while (q < end && *q != '"') {
        if (*q == '\\') return 0;
        q++;
    }
    if (q >= end) return 0;
    if ((size_t)(q - p) == klen && strncmp(p, key, klen) == 0) {
        *after_quote = q + 1;
        return 1;
    }
    return 0;
}

static const char *find_field_in_range(const char *start, const char *end, const char *key) {
    const char *p = start;
    while (p < end) {
        if (*p != '"') {
            p++;
            continue;
        }
        const char *after = NULL;
        if (raw_string_key_matches(p, end, key, &after)) {
            const char *q = skip_ws(after, end);
            if (q < end && *q == ':') {
                q++;
                return skip_ws(q, end);
            }
        }
        p = skip_string_raw(p, end);
        if (p == NULL) return NULL;
    }
    return NULL;
}

static int hex_digit(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return 10 + c - 'a';
    if (c >= 'A' && c <= 'F') return 10 + c - 'A';
    return -1;
}

static char *parse_json_string_at(const char **pp, const char *end) {
    const char *p = skip_ws(*pp, end);
    if (p >= end || *p != '"') return NULL;
    p++;
    Buf b;
    buf_init(&b);
    while (p < end) {
        unsigned char c = (unsigned char)*p;
        if (c == '"') {
            p++;
            *pp = p;
            return buf_steal(&b);
        }
        if (c == '\\') {
            p++;
            if (p >= end) break;
            switch (*p) {
                case '"':
                case '\\':
                case '/':
                    if (buf_append_char(&b, *p) != 0) goto oom;
                    break;
                case 'b':
                    if (buf_append_char(&b, '\b') != 0) goto oom;
                    break;
                case 'f':
                    if (buf_append_char(&b, '\f') != 0) goto oom;
                    break;
                case 'n':
                    if (buf_append_char(&b, '\n') != 0) goto oom;
                    break;
                case 'r':
                    if (buf_append_char(&b, '\r') != 0) goto oom;
                    break;
                case 't':
                    if (buf_append_char(&b, '\t') != 0) goto oom;
                    break;
                case 'u':
                    if (p + 4 >= end ||
                        hex_digit(p[1]) < 0 ||
                        hex_digit(p[2]) < 0 ||
                        hex_digit(p[3]) < 0 ||
                        hex_digit(p[4]) < 0) {
                        goto fail;
                    }
                    if (buf_append_char(&b, '?') != 0) goto oom;
                    p += 4;
                    break;
                default:
                    goto fail;
            }
        } else {
            if (buf_append_char(&b, (char)c) != 0) goto oom;
        }
        p++;
    }
fail:
    buf_free(&b);
    return NULL;
oom:
    buf_free(&b);
    return NULL;
}

static char *parse_string_field(const char *start, const char *end, const char *key) {
    const char *p = find_field_in_range(start, end, key);
    if (p == NULL) return NULL;
    return parse_json_string_at(&p, end);
}

static int parse_int_field(const char *start, const char *end, const char *key, int *out) {
    const char *p = find_field_in_range(start, end, key);
    char *stop = NULL;
    long value;
    if (p == NULL) return 0;
    errno = 0;
    value = strtol(p, &stop, 10);
    if (errno != 0 || stop == p || value < 0 || value > INT_MAX) return 0;
    *out = (int)value;
    return 1;
}

static StringArray parse_string_array_at(const char *p, const char *end, int *ok) {
    StringArray arr;
    string_array_init(&arr);
    *ok = 0;
    p = skip_ws(p, end);
    if (p >= end || *p != '[') return arr;
    p++;
    while (p < end) {
        p = skip_ws(p, end);
        if (p < end && *p == ']') {
            *ok = 1;
            return arr;
        }
        char *item = parse_json_string_at(&p, end);
        if (item == NULL) {
            string_array_free(&arr);
            return arr;
        }
        if (string_array_push(&arr, item) != 0) {
            free(item);
            string_array_free(&arr);
            return arr;
        }
        p = skip_ws(p, end);
        if (p < end && *p == ',') {
            p++;
            continue;
        }
        if (p < end && *p == ']') {
            *ok = 1;
            return arr;
        }
        string_array_free(&arr);
        return arr;
    }
    string_array_free(&arr);
    return arr;
}

static StringArray parse_string_array_field(const char *start, const char *end,
                                            const char *key, int *present) {
    int ok = 0;
    const char *p = find_field_in_range(start, end, key);
    StringArray arr;
    string_array_init(&arr);
    *present = 0;
    if (p == NULL) return arr;
    arr = parse_string_array_at(p, end, &ok);
    if (ok) *present = 1;
    return arr;
}

static char *raw_json_field(const char *start, const char *end, const char *key) {
    const char *p = find_field_in_range(start, end, key);
    const char *q;
    if (p == NULL) return NULL;
    q = json_value_end(p, end);
    if (q == NULL || q <= p) return NULL;
    while (p < q && isspace((unsigned char)*p)) p++;
    while (q > p && isspace((unsigned char)q[-1])) q--;
    return xstrndup(p, (size_t)(q - p));
}

static void json_escape_to_buf(Buf *out, const char *s) {
    for (const unsigned char *p = (const unsigned char *)s; p != NULL && *p; p++) {
        switch (*p) {
            case '"':
                buf_append(out, "\\\"");
                break;
            case '\\':
                buf_append(out, "\\\\");
                break;
            case '\b':
                buf_append(out, "\\b");
                break;
            case '\f':
                buf_append(out, "\\f");
                break;
            case '\n':
                buf_append(out, "\\n");
                break;
            case '\r':
                buf_append(out, "\\r");
                break;
            case '\t':
                buf_append(out, "\\t");
                break;
            default:
                if (*p < 0x20) {
                    char esc[7];
                    snprintf(esc, sizeof(esc), "\\u%04x", (unsigned)*p);
                    buf_append(out, esc);
                } else {
                    buf_append_char(out, (char)*p);
                }
                break;
        }
    }
}

static int buf_append_json_string(Buf *b, const char *s) {
    if (buf_append_char(b, '"') != 0) return -1;
    json_escape_to_buf(b, s == NULL ? "" : s);
    return buf_append_char(b, '"');
}

static char *json_quote(const char *s) {
    Buf b;
    buf_init(&b);
    buf_append_char(&b, '"');
    json_escape_to_buf(&b, s);
    buf_append_char(&b, '"');
    return buf_steal(&b);
}

static char *capture_id_literal(const char *json, const char *end) {
    const char *p = find_field_in_range(json, end, "id");
    const char *q;
    if (p == NULL) return xstrdup("null");
    q = json_value_end(p, end);
    if (q == NULL || q <= p) return xstrdup("null");
    while (q > p && isspace((unsigned char)q[-1])) q--;
    char *out = (char *)malloc((size_t)(q - p) + 1);
    if (out == NULL) return xstrdup("null");
    memcpy(out, p, (size_t)(q - p));
    out[q - p] = '\0';
    return out;
}

static void send_result(const char *id, const char *result_json) {
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":%s}\n",
           id != NULL ? id : "null",
           result_json);
    fflush(stdout);
}

static void send_error(const char *id, int code, const char *message) {
    char *quoted = json_quote(message);
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":{\"code\":%d,\"message\":%s}}\n",
           id != NULL ? id : "null",
           code,
           quoted != NULL ? quoted : "\"\"");
    fflush(stdout);
    free(quoted);
}

static char *cid_for_bytes(const char *data, size_t len) {
    uint8_t out[64];
    blake3_hasher hasher;
    const char prefix[] = "blake3-512:";
    size_t prefix_len = strlen(prefix);
    char *cid = (char *)malloc(prefix_len + sizeof(out) * 2 + 1);

    if (cid == NULL) return NULL;
    blake3_hasher_init(&hasher);
    blake3_hasher_update(&hasher, data == NULL ? "" : data, len);
    blake3_hasher_finalize(&hasher, out, sizeof(out));
    memcpy(cid, prefix, prefix_len);
    for (size_t i = 0; i < sizeof(out); i++) {
        static const char hex[] = "0123456789abcdef";

        cid[prefix_len + i * 2] = hex[out[i] >> 4];
        cid[prefix_len + i * 2 + 1] = hex[out[i] & 0x0f];
    }
    cid[prefix_len + sizeof(out) * 2] = '\0';
    return cid;
}

static int is_hex_char(char c) {
    return (c >= '0' && c <= '9') ||
           (c >= 'a' && c <= 'f') ||
           (c >= 'A' && c <= 'F');
}

static int is_valid_blake3_512_cid(const char *s) {
    const char prefix[] = "blake3-512:";
    size_t prefix_len = strlen(prefix);
    if (s == NULL || strncmp(s, prefix, prefix_len) != 0) return 0;
    if (strlen(s) != prefix_len + 128) return 0;
    for (size_t i = 0; i < 128; i++) {
        if (!is_hex_char(s[prefix_len + i])) return 0;
    }
    return 1;
}

static char *path_join(const char *base, const char *rel) {
    size_t blen = strlen(base);
    size_t rlen = strlen(rel);
    int needs_slash = blen > 0 && base[blen - 1] != '/';
    char *out = (char *)malloc(blen + (needs_slash ? 1U : 0U) + rlen + 1);
    if (out == NULL) return NULL;
    memcpy(out, base, blen);
    size_t pos = blen;
    if (needs_slash) out[pos++] = '/';
    memcpy(out + pos, rel, rlen);
    out[pos + rlen] = '\0';
    return out;
}

static int file_exists(const char *path) {
    return access(path, R_OK) == 0;
}

static char *read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    long size;
    char *data;
    size_t got;
    if (f == NULL) return NULL;
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    size = ftell(f);
    if (size < 0) {
        fclose(f);
        return NULL;
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }
    data = (char *)malloc((size_t)size + 1);
    if (data == NULL) {
        fclose(f);
        return NULL;
    }
    got = fread(data, 1, (size_t)size, f);
    fclose(f);
    if (got != (size_t)size) {
        free(data);
        return NULL;
    }
    data[got] = '\0';
    return data;
}

static char *dirname_copy(const char *path) {
    const char *slash = strrchr(path, '/');
    if (slash == NULL) return xstrdup(".");
    if (slash == path) return xstrdup("/");
    size_t len = (size_t)(slash - path);
    char *out = (char *)malloc(len + 1);
    if (out == NULL) return NULL;
    memcpy(out, path, len);
    out[len] = '\0';
    return out;
}

static char *absolute_dir_for_argv0(const char *argv0) {
    char cwd[PATH_MAX];
    char *dir;
    char *joined;
    if (argv0 == NULL || argv0[0] == '\0') return NULL;
    dir = dirname_copy(argv0);
    if (dir == NULL) return NULL;
    if (dir[0] == '/') return dir;
    if (getcwd(cwd, sizeof(cwd)) == NULL) {
        free(dir);
        return NULL;
    }
    joined = path_join(cwd, dir);
    free(dir);
    return joined;
}

static char *try_find_from_base(const char *base, const char *rel) {
    char *cursor = xstrdup(base);
    if (cursor == NULL) return NULL;
    while (cursor[0] != '\0') {
        char *candidate = path_join(cursor, rel);
        if (candidate != NULL && file_exists(candidate)) {
            free(cursor);
            return candidate;
        }
        free(candidate);
        char *slash = strrchr(cursor, '/');
        if (slash == NULL) break;
        if (slash == cursor) {
            cursor[1] = '\0';
            candidate = path_join(cursor, rel);
            if (candidate != NULL && file_exists(candidate)) {
                free(cursor);
                return candidate;
            }
            free(candidate);
            break;
        }
        *slash = '\0';
    }
    free(cursor);
    return NULL;
}

static char *find_body_template_path(const char *argv0) {
    const char *env_file = getenv("PROVEKIT_REALIZE_C_BODY_TEMPLATES");
    const char *env_root = getenv("PROVEKIT_REPO_ROOT");
    char cwd[PATH_MAX];
    char *candidate;
    char *bin_dir;
    if (env_file != NULL && env_file[0] != '\0' && file_exists(env_file)) {
        return xstrdup(env_file);
    }
    if (env_root != NULL && env_root[0] != '\0') {
        candidate = path_join(env_root, BODY_TEMPLATE_REL);
        if (candidate != NULL && file_exists(candidate)) return candidate;
        free(candidate);
    }
    if (getcwd(cwd, sizeof(cwd)) != NULL) {
        candidate = try_find_from_base(cwd, BODY_TEMPLATE_REL);
        if (candidate != NULL) return candidate;
    }
    bin_dir = absolute_dir_for_argv0(argv0);
    if (bin_dir != NULL) {
        candidate = try_find_from_base(bin_dir, BODY_TEMPLATE_REL);
        free(bin_dir);
        if (candidate != NULL) return candidate;
    }
    return NULL;
}

static void parse_entry(const char *start, const char *end, TemplateCatalog *catalog) {
    BodyTemplateEntry entry;
    const char *template_obj;
    const char *template_end;
    int min_value = 0;
    int max_value = 0;
    int present = 0;
    memset(&entry, 0, sizeof(entry));
    string_array_init(&entry.requires_param_types);

    entry.concept_name = parse_string_field(start, end, "concept_name");
    if (entry.concept_name == NULL) goto skip;

    template_obj = find_field_in_range(start, end, "emission_template");
    if (template_obj == NULL || *template_obj != '{') goto skip;
    template_end = json_value_end(template_obj, end);
    if (template_end == NULL) goto skip;
    entry.template_kind = parse_string_field(template_obj, template_end, "kind");
    entry.template_text = parse_string_field(template_obj, template_end, "template");
    if (entry.template_kind == NULL || entry.template_text == NULL) goto skip;

    if (parse_int_field(start, end, "min_params", &min_value)) {
        entry.has_min_params = 1;
        entry.min_params = min_value;
    }
    if (parse_int_field(start, end, "max_params", &max_value)) {
        entry.has_max_params = 1;
        entry.max_params = max_value;
    }
    entry.requires_return_type = parse_string_field(start, end, "requires_return_type");
    entry.requires_param_types = parse_string_array_field(start, end, "requires_param_types", &present);
    entry.has_requires_param_types = present;

    if (catalog_push(catalog, entry) == 0) return;

skip:
    entry_free(&entry);
}

static TemplateCatalog load_catalog(const char *argv0) {
    TemplateCatalog catalog;
    char *path;
    char *raw;
    const char *start;
    const char *end;
    const char *entries;
    const char *array_end;
    catalog_init(&catalog);
    path = find_body_template_path(argv0);
    if (path == NULL) return catalog;
    raw = read_file(path);
    free(path);
    if (raw == NULL) return catalog;
    start = raw;
    end = raw + strlen(raw);
    entries = find_field_in_range(start, end, "entries");
    if (entries == NULL || *entries != '[') {
        free(raw);
        return catalog;
    }
    array_end = json_value_end(entries, end);
    if (array_end == NULL) {
        free(raw);
        return catalog;
    }
    const char *p = entries + 1;
    while (p < array_end) {
        p = skip_ws(p, array_end);
        if (p >= array_end || *p == ']') break;
        if (*p == ',') {
            p++;
            continue;
        }
        if (*p == '{') {
            const char *obj_end = json_value_end(p, array_end);
            if (obj_end == NULL) break;
            parse_entry(p, obj_end, &catalog);
            p = obj_end;
            continue;
        }
        p++;
    }
    free(raw);
    return catalog;
}

static int is_passthrough_c_type(const char *src) {
    if (src == NULL || src[0] == '\0') return 0;
    for (const unsigned char *p = (const unsigned char *)src; *p; p++) {
        if (isalnum(*p) || *p == '_' || *p == '*' || isspace(*p)) continue;
        return 0;
    }
    if (strcmp(src, "char") == 0 ||
        strcmp(src, "signed char") == 0 ||
        strcmp(src, "unsigned char") == 0 ||
        strcmp(src, "short") == 0 ||
        strcmp(src, "unsigned short") == 0 ||
        strcmp(src, "int") == 0 ||
        strcmp(src, "unsigned int") == 0 ||
        strcmp(src, "long") == 0 ||
        strcmp(src, "unsigned long") == 0 ||
        strcmp(src, "long long") == 0 ||
        strcmp(src, "unsigned long long") == 0 ||
        strcmp(src, "size_t") == 0 ||
        strcmp(src, "float") == 0 ||
        strcmp(src, "double") == 0 ||
        strcmp(src, "long double") == 0 ||
        strcmp(src, "bool") == 0 ||
        strcmp(src, "_Bool") == 0 ||
        strcmp(src, "void") == 0) {
        return 1;
    }
    if (strncmp(src, "struct ", 7) == 0 ||
        strncmp(src, "enum ", 5) == 0 ||
        strncmp(src, "union ", 6) == 0) {
        return 1;
    }
    if (strchr(src, '*') != NULL &&
        (strstr(src, "char") != NULL ||
         strstr(src, "short") != NULL ||
         strstr(src, "int") != NULL ||
         strstr(src, "long") != NULL ||
         strstr(src, "size_t") != NULL ||
         strstr(src, "float") != NULL ||
         strstr(src, "double") != NULL ||
         strstr(src, "bool") != NULL ||
         strstr(src, "_Bool") != NULL ||
         strstr(src, "void") != NULL)) {
        return 1;
    }
    return 0;
}

static char *map_source_type(const char *src) {
    if (src == NULL || src[0] == '\0') return NULL;
    if (strcmp(src, "Unit") == 0 || strcmp(src, "()") == 0) return xstrdup("void");
    if (strcmp(src, "Int") == 0) return xstrdup("int");
    if (strcmp(src, "Bool") == 0 || strcmp(src, "Boolean") == 0) return xstrdup("bool");
    if (strcmp(src, "Float") == 0) return xstrdup("float");
    if (strcmp(src, "Real") == 0) return xstrdup("double");
    if (strcmp(src, "String") == 0) return xstrdup("const char*");
    if (strcmp(src, "()") == 0) return xstrdup("void");
    if (strcmp(src, "void") == 0) return xstrdup("void");
    if (strcmp(src, "i64") == 0 || strcmp(src, "u64") == 0) return xstrdup("long");
    if (strcmp(src, "isize") == 0) return xstrdup("long");
    if (strcmp(src, "usize") == 0) return xstrdup("size_t");
    if (strcmp(src, "i32") == 0 || strcmp(src, "u32") == 0 || strcmp(src, "int") == 0) {
        return xstrdup("int");
    }
    if (strcmp(src, "i16") == 0 || strcmp(src, "u16") == 0) return xstrdup("short");
    if (strcmp(src, "i8") == 0 || strcmp(src, "u8") == 0) return xstrdup("char");
    if (strcmp(src, "f64") == 0) return xstrdup("double");
    if (strcmp(src, "f32") == 0) return xstrdup("float");
    if (strcmp(src, "bool") == 0) return xstrdup("bool");
    if (strcmp(src, "String") == 0 || strcmp(src, "&str") == 0 ||
        strcmp(src, "&String") == 0 || strcmp(src, "str") == 0) {
        return xstrdup("const char*");
    }
    if (is_passthrough_c_type(src)) return xstrdup(src);
    return NULL;
}

static int concept_matches(const char *entry_name, const char *request_name) {
    const char *prefix = "concept:";
    size_t prefix_len = strlen(prefix);
    if (strcmp(entry_name, request_name) == 0) return 1;
    if (strncmp(entry_name, prefix, prefix_len) == 0 &&
        strcmp(entry_name + prefix_len, request_name) == 0) {
        return 1;
    }
    if (strncmp(request_name, prefix, prefix_len) == 0 &&
        strcmp(request_name + prefix_len, entry_name) == 0) {
        return 1;
    }
    return 0;
}

static int replace_all(Buf *out, const char *src, const char *needle, const char *replacement) {
    size_t nlen = strlen(needle);
    const char *p = src;
    const char *hit;
    while ((hit = strstr(p, needle)) != NULL) {
        if (buf_append_n(out, p, (size_t)(hit - p)) != 0) return -1;
        if (buf_append(out, replacement) != 0) return -1;
        p = hit + nlen;
    }
    return buf_append(out, p);
}

static char *replace_all_owned(char *src, const char *needle, const char *replacement) {
    Buf out;
    char *rendered;
    buf_init(&out);
    if (replace_all(&out, src, needle, replacement) != 0) {
        buf_free(&out);
        free(src);
        return NULL;
    }
    rendered = buf_steal(&out);
    free(src);
    return rendered;
}

static char *render_template(const BodyTemplateEntry *entry, const StringArray *params,
                             const StringArray *mapped_param_types,
                             const char *mapped_return_type) {
    char *rendered = xstrdup(entry->template_text);
    char needle[64];
    char count_buf[32];
    if (rendered == NULL) return NULL;
    for (size_t i = 0; i < params->len; i++) {
        snprintf(needle, sizeof(needle), "${param%zu}", i);
        rendered = replace_all_owned(rendered, needle, params->items[i]);
        if (rendered == NULL) return NULL;
    }
    for (size_t i = 0; i < mapped_param_types->len; i++) {
        snprintf(needle, sizeof(needle), "${param_type_%zu}", i);
        rendered = replace_all_owned(rendered, needle, mapped_param_types->items[i]);
        if (rendered == NULL) return NULL;
    }
    snprintf(count_buf, sizeof(count_buf), "%zu", params->len);
    rendered = replace_all_owned(rendered, "${param_count}", count_buf);
    if (rendered == NULL) return NULL;
    rendered = replace_all_owned(rendered, "${return_type}", mapped_return_type);
    if (rendered == NULL) return NULL;
    if (strstr(rendered, "${") != NULL) {
        free(rendered);
        return NULL;
    }
    return rendered;
}

static int entry_signature_matches(const BodyTemplateEntry *entry, const StringArray *params,
                                   const StringArray *mapped_param_types,
                                   const char *mapped_return_type) {
    if (entry->has_min_params && params->len < (size_t)entry->min_params) return 0;
    if (entry->has_max_params && params->len > (size_t)entry->max_params) return 0;
    if (entry->requires_return_type != NULL &&
        strcmp(entry->requires_return_type, mapped_return_type) != 0) {
        return 0;
    }
    if (entry->has_requires_param_types) {
        if (entry->requires_param_types.len != mapped_param_types->len) return 0;
        for (size_t i = 0; i < mapped_param_types->len; i++) {
            if (strcmp(entry->requires_param_types.items[i], mapped_param_types->items[i]) != 0) {
                return 0;
            }
        }
    }
    return strcmp(entry->template_kind, "verbatim") == 0;
}

static char *body_template_for(const TemplateCatalog *catalog, const char *concept_name,
                               const StringArray *params,
                               const StringArray *mapped_param_types,
                               const char *mapped_return_type) {
    char *rendered = NULL;
    for (size_t i = 0; i < catalog->len; i++) {
        const BodyTemplateEntry *entry = &catalog->entries[i];
        if (!concept_matches(entry->concept_name, concept_name)) continue;
        if (!entry_signature_matches(entry, params, mapped_param_types, mapped_return_type)) {
            continue;
        }
        rendered = render_template(entry, params, mapped_param_types, mapped_return_type);
        if (rendered != NULL) break;
    }
    return rendered;
}

static char *unsupported_sort_message(const char *sort) {
    Buf b;
    buf_init(&b);
    if (buf_append(&b, "UNSUPPORTED_SORT: no C type mapping for ") != 0 ||
        buf_append(&b, sort != NULL ? sort : "") != 0) {
        buf_free(&b);
        return NULL;
    }
    return buf_steal(&b);
}

static int map_signature_types(const StringArray *param_types, const char *return_type,
                               StringArray *mapped_param_types,
                               char **mapped_return_type,
                               char **error_message) {
    *mapped_return_type = map_source_type(return_type);
    if (*mapped_return_type == NULL) {
        *error_message = unsupported_sort_message(return_type);
        return *error_message == NULL ? -2 : -1;
    }
    for (size_t i = 0; i < param_types->len; i++) {
        char *mapped = map_source_type(param_types->items[i]);
        if (mapped == NULL) {
            *error_message = unsupported_sort_message(param_types->items[i]);
            return *error_message == NULL ? -2 : -1;
        }
        if (strcmp(mapped, "void") == 0) {
            free(mapped);
            *error_message = unsupported_sort_message(param_types->items[i]);
            return *error_message == NULL ? -2 : -1;
        }
        if (string_array_push(mapped_param_types, mapped) != 0) {
            free(mapped);
            return -2;
        }
    }
    return 0;
}

static char *indent_body(const char *body);

static int append_typed_param(Buf *out, const char *type, const char *name) {
    size_t len = strlen(type);
    if (buf_append(out, type) != 0) return -1;
    if (!(len >= 2 && type[len - 1] == '*' && isspace((unsigned char)type[len - 2]))) {
        if (buf_append_char(out, ' ') != 0) return -1;
    }
    return buf_append(out, name);
}

static int append_field_key(Buf *out, int *first, const char *key) {
    if (!*first && buf_append_char(out, ',') != 0) return -1;
    *first = 0;
    return buf_append_json_string(out, key) == 0 && buf_append_char(out, ':') == 0 ? 0 : -1;
}

static int append_string_field(Buf *out, int *first, const char *key, const char *value) {
    return append_field_key(out, first, key) == 0 &&
        buf_append_json_string(out, value) == 0 ? 0 : -1;
}

static int append_raw_field(Buf *out, int *first, const char *key, const char *value) {
    return append_field_key(out, first, key) == 0 &&
        buf_append(out, value) == 0 ? 0 : -1;
}

static int append_emitted_by_field(Buf *out, int *first, const char *kit_cid,
                                   const char *kit_id, const char *target_library_tag) {
    int emitted_first = 1;
    if (append_field_key(out, first, "emitted_by") != 0 ||
        buf_append_char(out, '{') != 0 ||
        append_string_field(out, &emitted_first, "kit_cid", kit_cid) != 0 ||
        append_string_field(out, &emitted_first, "kit_id", kit_id) != 0 ||
        append_string_field(out, &emitted_first, "kit_kind", "realize") != 0 ||
        append_string_field(out, &emitted_first, "target_language", "c") != 0 ||
        append_string_field(out, &emitted_first, "target_library_tag", target_library_tag) != 0 ||
        buf_append_char(out, '}') != 0) {
        return -1;
    }
    return 0;
}

static const char *concept_display_name(const char *concept_name) {
    const char *prefix = "concept:";
    size_t prefix_len = strlen(prefix);
    if (concept_name != NULL && strncmp(concept_name, prefix, prefix_len) == 0) {
        return concept_name + prefix_len;
    }
    return concept_name != NULL ? concept_name : "unknown";
}

static int append_concept_shorthand(Buf *out, const char *concept_name,
                                    const StringArray *params) {
    if (buf_append(out, "/* concept: ") != 0 ||
        buf_append(out, concept_display_name(concept_name)) != 0 ||
        buf_append_char(out, '(') != 0) {
        return -1;
    }
    for (size_t i = 0; i < params->len; i++) {
        if (i > 0 && buf_append(out, ", ") != 0) return -1;
        if (buf_append(out, params->items[i]) != 0) return -1;
    }
    return buf_append(out, ") */\n");
}

static char *concept_citation_body_for(const char *params_obj, const char *params_obj_end,
                                       const char *fallback_concept_name,
                                       const StringArray *params,
                                       char **error_message) {
    const char *op = find_field_in_range(params_obj, params_obj_end, "transported_operation");
    const char *op_end;
    char *args_jcs = NULL;
    char *args_jcs_cid = NULL;
    char *callsite_cid = NULL;
    char *concept_cid = NULL;
    char *concept_name = NULL;
    char *concept_site_cid = NULL;
    char *kit_cid = NULL;
    char *kit_id = NULL;
    char *loss_record_cid = NULL;
    char *operation_kind = NULL;
    char *policy_cid = NULL;
    char *shape_cid = NULL;
    char *sugar_dict_cid = NULL;
    char *target_library_tag = NULL;
    char *term_position = NULL;
    char *payload = NULL;
    char *payload_cid = NULL;
    char *body = NULL;
    Buf payload_buf;
    Buf body_buf;
    int first = 1;

    if (op == NULL) return NULL;
    if (*op != '{') {
        *error_message = xstrdup("INVALID_PARAMS: transported_operation must be an object");
        return NULL;
    }
    op_end = json_value_end(op, params_obj_end);
    if (op_end == NULL) {
        *error_message = xstrdup("INVALID_PARAMS: malformed transported_operation");
        return NULL;
    }

    args_jcs = raw_json_field(op, op_end, "args_jcs");
    args_jcs_cid = args_jcs == NULL
        ? parse_string_field(op, op_end, "args_jcs_cid")
        : cid_for_bytes(args_jcs, strlen(args_jcs));
    callsite_cid = parse_string_field(op, op_end, "callsite_cid");
    concept_cid = parse_string_field(op, op_end, "concept_cid");
    concept_name = parse_string_field(op, op_end, "concept_name");
    if (concept_name == NULL && fallback_concept_name != NULL) {
        concept_name = xstrdup(fallback_concept_name);
    }
    concept_site_cid = parse_string_field(op, op_end, "concept_site_cid");
    kit_id = parse_string_field(op, op_end, "kit_id");
    if (kit_id == NULL) kit_id = xstrdup(DEFAULT_KIT_ID);
    kit_cid = parse_string_field(op, op_end, "kit_cid");
    if (kit_cid == NULL && kit_id != NULL) kit_cid = cid_for_bytes(kit_id, strlen(kit_id));
    loss_record_cid = parse_string_field(op, op_end, "loss_record_cid");
    operation_kind = parse_string_field(op, op_end, "operation_kind");
    policy_cid = parse_string_field(op, op_end, "policy_cid");
    shape_cid = parse_string_field(op, op_end, "shape_cid");
    sugar_dict_cid = parse_string_field(op, op_end, "sugar_dict_cid");
    target_library_tag = parse_string_field(op, op_end, "target_library_tag");
    if (target_library_tag == NULL) target_library_tag = xstrdup(DEFAULT_TARGET_LIBRARY_TAG);
    term_position = raw_json_field(op, op_end, "term_position");

    if (concept_cid == NULL || concept_site_cid == NULL || kit_cid == NULL ||
        kit_id == NULL || loss_record_cid == NULL || operation_kind == NULL ||
        shape_cid == NULL || sugar_dict_cid == NULL || target_library_tag == NULL ||
        term_position == NULL || args_jcs_cid == NULL) {
        *error_message = xstrdup("INVALID_PARAMS: missing transported_operation field");
        goto done;
    }
    if (term_position[0] != '[') {
        *error_message = xstrdup("INVALID_PARAMS: transported_operation term_position must be an array");
        goto done;
    }
    if (!is_valid_blake3_512_cid(args_jcs_cid) ||
        (callsite_cid != NULL && !is_valid_blake3_512_cid(callsite_cid)) ||
        !is_valid_blake3_512_cid(concept_cid) ||
        !is_valid_blake3_512_cid(concept_site_cid) ||
        !is_valid_blake3_512_cid(kit_cid) ||
        !is_valid_blake3_512_cid(loss_record_cid) ||
        (policy_cid != NULL && !is_valid_blake3_512_cid(policy_cid)) ||
        !is_valid_blake3_512_cid(shape_cid) ||
        !is_valid_blake3_512_cid(sugar_dict_cid)) {
        *error_message = xstrdup("INVALID_PARAMS: malformed transported_operation CID");
        goto done;
    }

    buf_init(&payload_buf);
    if (buf_append_char(&payload_buf, '{') != 0 ||
        (args_jcs != NULL && append_raw_field(&payload_buf, &first, "args_jcs", args_jcs) != 0) ||
        append_string_field(&payload_buf, &first, "args_jcs_cid", args_jcs_cid) != 0 ||
        append_string_field(&payload_buf, &first, "artifact_kind", CONCEPT_CITATION_COMMENT_KIND) != 0 ||
        (callsite_cid != NULL && append_string_field(&payload_buf, &first, "callsite_cid", callsite_cid) != 0) ||
        append_string_field(&payload_buf, &first, "concept_cid", concept_cid) != 0 ||
        (concept_name != NULL && append_string_field(&payload_buf, &first, "concept_name", concept_name) != 0) ||
        append_string_field(&payload_buf, &first, "concept_site_cid", concept_site_cid) != 0 ||
        append_emitted_by_field(&payload_buf, &first, kit_cid, kit_id, target_library_tag) != 0 ||
        append_string_field(&payload_buf, &first, "loss_record_cid", loss_record_cid) != 0 ||
        append_string_field(&payload_buf, &first, "operation_kind", operation_kind) != 0 ||
        (policy_cid != NULL && append_string_field(&payload_buf, &first, "policy_cid", policy_cid) != 0) ||
        append_string_field(&payload_buf, &first, "schema_version", "1") != 0 ||
        append_string_field(&payload_buf, &first, "shape_cid", shape_cid) != 0 ||
        append_string_field(&payload_buf, &first, "sugar_dict_cid", sugar_dict_cid) != 0 ||
        append_raw_field(&payload_buf, &first, "term_position", term_position) != 0 ||
        buf_append_char(&payload_buf, '}') != 0) {
        buf_free(&payload_buf);
        *error_message = xstrdup("out of memory");
        goto done;
    }
    payload = buf_steal(&payload_buf);
    payload_cid = cid_for_bytes(payload, strlen(payload));
    if (payload == NULL || payload_cid == NULL) {
        *error_message = xstrdup("out of memory");
        goto done;
    }

    buf_init(&body_buf);
    if (append_concept_shorthand(&body_buf, concept_name, params) != 0 ||
        buf_append(&body_buf, "// provekit-concept: ") != 0 ||
        buf_append(&body_buf, payload) != 0 ||
        buf_append(&body_buf, "\n// provekit-concept-payload-cid: ") != 0 ||
        buf_append(&body_buf, payload_cid) != 0) {
        buf_free(&body_buf);
        *error_message = xstrdup("out of memory");
        goto done;
    }
    for (size_t i = 0; i < params->len; i++) {
        if (buf_append(&body_buf, "\n(void)") != 0 ||
            buf_append(&body_buf, params->items[i]) != 0 ||
            buf_append_char(&body_buf, ';') != 0) {
            buf_free(&body_buf);
            *error_message = xstrdup("out of memory");
            goto done;
        }
    }
    if (buf_append(&body_buf, "\n(void)0;") != 0) {
        buf_free(&body_buf);
        *error_message = xstrdup("out of memory");
        goto done;
    }
    body = buf_steal(&body_buf);

done:
    free(args_jcs);
    free(args_jcs_cid);
    free(callsite_cid);
    free(concept_cid);
    free(concept_name);
    free(concept_site_cid);
    free(kit_cid);
    free(kit_id);
    free(loss_record_cid);
    free(operation_kind);
    free(policy_cid);
    free(shape_cid);
    free(sugar_dict_cid);
    free(target_library_tag);
    free(term_position);
    free(payload);
    free(payload_cid);
    return body;
}

static char *function_source(const char *function, const StringArray *params,
                             const StringArray *mapped_param_types,
                             const char *mapped_return_type, const char *body) {
    Buf out;
    char *indented_body = indent_body(body);
    if (indented_body == NULL) return NULL;
    buf_init(&out);
    if (buf_append(&out, mapped_return_type) != 0 ||
        buf_append_char(&out, ' ') != 0 ||
        buf_append(&out, function) != 0 ||
        buf_append_char(&out, '(') != 0) {
        goto oom;
    }
    if (params->len == 0) {
        if (buf_append(&out, "void") != 0) goto oom;
    } else {
        for (size_t i = 0; i < params->len; i++) {
            if (i > 0 && buf_append(&out, ", ") != 0) goto oom;
            if (append_typed_param(&out, mapped_param_types->items[i], params->items[i]) != 0) {
                goto oom;
            }
        }
    }
    if (buf_append(&out, ") {\n") != 0 ||
        buf_append(&out, indented_body) != 0 ||
        buf_append(&out, "}\n") != 0) {
        goto oom;
    }
    free(indented_body);
    return buf_steal(&out);

oom:
    free(indented_body);
    buf_free(&out);
    return NULL;
}

static char *indent_body(const char *body) {
    Buf out;
    const char *p = body;
    buf_init(&out);
    if (body[0] == '\0') {
        buf_append(&out, "    \n");
        return buf_steal(&out);
    }
    while (*p != '\0') {
        const char *line_start = p;
        const char *line_end = strchr(p, '\n');
        buf_append(&out, "    ");
        if (line_end == NULL) {
            buf_append(&out, line_start);
            buf_append_char(&out, '\n');
            break;
        }
        buf_append_n(&out, line_start, (size_t)(line_end - line_start));
        buf_append_char(&out, '\n');
        p = line_end + 1;
        if (*p == '\0') break;
    }
    return buf_steal(&out);
}

static int return_type_is_void(const char *return_type) {
    return strcmp(return_type, "void") == 0 ||
           strcmp(return_type, "()") == 0 ||
           strcmp(return_type, "Unit") == 0;
}

static char *stub_body_for(const char *concept_name, const char *return_type) {
    Buf b;
    buf_init(&b);
    buf_append(&b, "/* provekit-bind canonical: ");
    buf_append(&b, concept_name);
    buf_append(&b, " */\n");
    if (return_type_is_void(return_type)) {
        buf_append(&b, "return;");
    } else {
        buf_append(&b, "return 0;");
    }
    return buf_steal(&b);
}

static void handle_initialize(const char *id) {
    send_result(id,
        "{\"name\":\"provekit-realize-c\","
        "\"version\":\"0.1.0\","
        "\"protocol_version\":\"pep/1.7.0\","
        "\"capabilities\":{"
        "\"authoring_surfaces\":[\"c\",\"c11\"],"
        "\"emits_signed_mementos\":false,"
        "\"ir_version\":\"v1.1.0\""
        "}}");
}

static void handle_invoke(const char *id, const char *line, const char *end,
                          const TemplateCatalog *catalog) {
    const char *params_obj = find_field_in_range(line, end, "params");
    const char *params_obj_end;
    char *function = NULL;
    char *return_type = NULL;
    char *concept_name = NULL;
    StringArray params;
    StringArray param_types;
    StringArray mapped_param_types;
    int params_present = 0;
    int param_types_present = 0;
    char *mapped_return_type = NULL;
    char *error_message = NULL;
    char *body = NULL;
    char *source = NULL;
    char *source_json = NULL;
    int is_stub = 0;
    string_array_init(&params);
    string_array_init(&param_types);
    string_array_init(&mapped_param_types);
    if (params_obj == NULL || *params_obj != '{') {
        send_error(id, -32602, "INVALID_PARAMS: params must be an object");
        return;
    }
    params_obj_end = json_value_end(params_obj, end);
    if (params_obj_end == NULL) {
        send_error(id, -32602, "INVALID_PARAMS: malformed params object");
        return;
    }
    function = parse_string_field(params_obj, params_obj_end, "function");
    return_type = parse_string_field(params_obj, params_obj_end, "return_type");
    concept_name = parse_string_field(params_obj, params_obj_end, "concept_name");
    params = parse_string_array_field(params_obj, params_obj_end, "params", &params_present);
    param_types = parse_string_array_field(params_obj, params_obj_end, "param_types", &param_types_present);
    if (function == NULL || return_type == NULL || concept_name == NULL ||
        !params_present || !param_types_present) {
        send_error(id, -32602, "INVALID_PARAMS: missing function, signature, or concept_name");
        goto done;
    }
    if (params.len != param_types.len) {
        send_error(id, -32602, "INVALID_PARAMS: params and param_types length mismatch");
        goto done;
    }
    int map_status = map_signature_types(&param_types, return_type, &mapped_param_types,
                                         &mapped_return_type, &error_message);
    if (map_status != 0) {
        send_error(id, map_status == -2 ? -32603 : -32602,
                   error_message != NULL ? error_message : "out of memory");
        goto done;
    }
    body = concept_citation_body_for(params_obj, params_obj_end, concept_name, &params,
                                     &error_message);
    if (error_message != NULL) {
        send_error(id, -32602, error_message);
        goto done;
    }
    if (body == NULL) {
        body = body_template_for(catalog, concept_name, &params, &mapped_param_types,
                                 mapped_return_type);
    }
    if (body == NULL) {
        body = stub_body_for(concept_name, mapped_return_type);
        is_stub = 1;
    }
    if (body == NULL) {
        send_error(id, -32603, "out of memory");
        goto done;
    }
    source = function_source(function, &params, &mapped_param_types, mapped_return_type, body);
    if (source == NULL) {
        send_error(id, -32603, "out of memory");
        goto done;
    }
    source_json = json_quote(source);
    if (source_json == NULL) {
        send_error(id, -32603, "out of memory");
        goto done;
    }
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":{\"source\":%s,\"is_stub\":%s}}\n",
           id != NULL ? id : "null",
           source_json,
           is_stub ? "true" : "false");
    fflush(stdout);

done:
    free(function);
    free(return_type);
    free(concept_name);
    string_array_free(&params);
    string_array_free(&param_types);
    string_array_free(&mapped_param_types);
    free(mapped_return_type);
    free(error_message);
    free(body);
    free(source);
    free(source_json);
}

static int run_rpc(const char *argv0) {
    TemplateCatalog catalog = load_catalog(argv0);
    char line[MAX_LINE];
    while (fgets(line, sizeof(line), stdin) != NULL) {
        size_t n = strlen(line);
        const char *end;
        char *id;
        char *method;
        const char *method_p;
        while (n > 0 && (line[n - 1] == '\n' || line[n - 1] == '\r')) {
            line[--n] = '\0';
        }
        if (n == 0) continue;
        end = line + n;
        id = capture_id_literal(line, end);
        method_p = find_field_in_range(line, end, "method");
        method = method_p == NULL ? NULL : parse_json_string_at(&method_p, end);
        if (method == NULL) {
            send_error(id, -32700, "parse error: missing method");
        } else if (strcmp(method, "initialize") == 0) {
            handle_initialize(id);
        } else if (strcmp(method, "provekit.plugin.invoke") == 0) {
            handle_invoke(id, line, end, &catalog);
        } else if (strcmp(method, "shutdown") == 0 ||
                   strcmp(method, "provekit.plugin.shutdown") == 0) {
            send_result(id, "null");
            free(method);
            free(id);
            break;
        } else {
            send_error(id, -32601, "METHOD_NOT_FOUND");
        }
        free(method);
        free(id);
    }
    catalog_free(&catalog);
    return 0;
}

int main(int argc, char **argv) {
    (void)argc;
    return run_rpc(argv != NULL ? argv[0] : NULL);
}
