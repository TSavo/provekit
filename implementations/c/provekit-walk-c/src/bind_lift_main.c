#define _POSIX_C_SOURCE 200809L

#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "blake3.h"

#ifdef PK_C_ENABLE_CLANG_AST
#include <clang-c/Index.h>
#endif

#define CONCEPT_CITATION_COMMENT_KIND "provekit-concept-citation-comment-sugar"

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} Buf;

typedef struct {
    char **items;
    size_t len;
} StringArray;

typedef struct {
    char *template_cid;
    char *concept_name_json;
    char *library_tag_json;
    char *family_json;
    char *contract_cid_json;
} BindingTemplate;

typedef struct {
    BindingTemplate *items;
    size_t len;
} BindingList;

typedef struct {
    Buf ir;
    Buf concept_citations;
    Buf diagnostics;
    size_t ir_count;
    size_t concept_citation_count;
    size_t diagnostic_count;
    char error[512];
} LiftAccumulator;

static void string_array_free(StringArray *arr);

static void buf_init(Buf *b) {
    b->len = 0;
    b->cap = 256;
    b->data = malloc(b->cap);
    if (b->data != NULL) {
        b->data[0] = '\0';
    } else {
        b->cap = 0;
    }
}

static void buf_free(Buf *b) {
    free(b->data);
    b->data = NULL;
    b->len = 0;
    b->cap = 0;
}

static char *buf_take(Buf *b) {
    char *data = b->data;

    b->data = NULL;
    b->len = 0;
    b->cap = 0;
    return data;
}

static int buf_grow(Buf *b, size_t need) {
    size_t next = b->cap ? b->cap : 256;
    char *data;

    while (next < b->len + need + 1) {
        if (next > ((size_t)-1) / 2) {
            return -1;
        }
        next *= 2;
    }

    data = realloc(b->data, next);
    if (data == NULL) {
        return -1;
    }
    b->data = data;
    b->cap = next;
    return 0;
}

static int buf_append_n(Buf *b, const char *s, size_t n) {
    if (buf_grow(b, n) != 0) {
        return -1;
    }
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

static int buf_append_uint(Buf *b, unsigned value) {
    char tmp[32];

    (void)snprintf(tmp, sizeof(tmp), "%u", value);
    return buf_append(b, tmp);
}

static int buf_append_json_string(Buf *b, const char *s) {
    if (buf_append_char(b, '"') != 0) return -1;
    for (const unsigned char *p = (const unsigned char *)(s == NULL ? "" : s); *p; p++) {
        switch (*p) {
        case '"':
            if (buf_append(b, "\\\"") != 0) return -1;
            break;
        case '\\':
            if (buf_append(b, "\\\\") != 0) return -1;
            break;
        case '\n':
            if (buf_append(b, "\\n") != 0) return -1;
            break;
        case '\r':
            if (buf_append(b, "\\r") != 0) return -1;
            break;
        case '\t':
            if (buf_append(b, "\\t") != 0) return -1;
            break;
        default:
            if (*p < 0x20) {
                char esc[7];

                (void)snprintf(esc, sizeof(esc), "\\u%04x", *p);
                if (buf_append(b, esc) != 0) return -1;
            } else if (buf_append_char(b, (char)*p) != 0) {
                return -1;
            }
            break;
        }
    }
    return buf_append_char(b, '"');
}

static char *copy_n(const char *src, size_t len) {
    char *out = malloc(len + 1);

    if (out == NULL) {
        return NULL;
    }
    memcpy(out, src, len);
    out[len] = '\0';
    return out;
}

static char *copy_string(const char *src) {
    return copy_n(src == NULL ? "" : src, strlen(src == NULL ? "" : src));
}

static void json_skip_ws(const char **p) {
    while (**p == ' ' || **p == '\t' || **p == '\r' || **p == '\n') {
        (*p)++;
    }
}

static int json_parse_value(const char **p);

static int json_hex_value(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static int json_decode_hex4(const char *p, unsigned *out) {
    unsigned value = 0;

    for (int i = 0; i < 4; i++) {
        int digit = json_hex_value(p[i]);
        if (digit < 0) return -1;
        value = (value << 4) | (unsigned)digit;
    }
    *out = value;
    return 0;
}

static int json_append_utf8(char *out, size_t cap, size_t *len, unsigned codepoint) {
    if (codepoint == 0 || codepoint > 0x10ffffu ||
        (codepoint >= 0xd800u && codepoint <= 0xdfffu)) {
        return -1;
    }
    if (codepoint < 0x80u) {
        if (*len + 1 >= cap) return -1;
        out[(*len)++] = (char)codepoint;
    } else if (codepoint < 0x800u) {
        if (*len + 2 >= cap) return -1;
        out[(*len)++] = (char)(0xc0u | (codepoint >> 6));
        out[(*len)++] = (char)(0x80u | (codepoint & 0x3fu));
    } else if (codepoint < 0x10000u) {
        if (*len + 3 >= cap) return -1;
        out[(*len)++] = (char)(0xe0u | (codepoint >> 12));
        out[(*len)++] = (char)(0x80u | ((codepoint >> 6) & 0x3fu));
        out[(*len)++] = (char)(0x80u | (codepoint & 0x3fu));
    } else {
        if (*len + 4 >= cap) return -1;
        out[(*len)++] = (char)(0xf0u | (codepoint >> 18));
        out[(*len)++] = (char)(0x80u | ((codepoint >> 12) & 0x3fu));
        out[(*len)++] = (char)(0x80u | ((codepoint >> 6) & 0x3fu));
        out[(*len)++] = (char)(0x80u | (codepoint & 0x3fu));
    }
    return 0;
}

static char *decode_json_string(const char *start, const char **end_out) {
    size_t cap = strlen(start) + 1;
    char *out = malloc(cap);
    size_t len = 0;
    const char *p = start;

    if (out == NULL) {
        return NULL;
    }
    while (*p != '\0' && *p != '"') {
        if (*p == '\\') {
            p++;
            switch (*p) {
            case 'b':
                out[len++] = '\b';
                p++;
                break;
            case 'f':
                out[len++] = '\f';
                p++;
                break;
            case 'n':
                out[len++] = '\n';
                p++;
                break;
            case 'r':
                out[len++] = '\r';
                p++;
                break;
            case 't':
                out[len++] = '\t';
                p++;
                break;
            case '"':
                out[len++] = '"';
                p++;
                break;
            case '\\':
                out[len++] = '\\';
                p++;
                break;
            case '/':
                out[len++] = '/';
                p++;
                break;
            case 'u': {
                unsigned codepoint;

                p++;
                if (json_decode_hex4(p, &codepoint) != 0) {
                    free(out);
                    return NULL;
                }
                p += 4;
                if (codepoint >= 0xd800u && codepoint <= 0xdbffu) {
                    unsigned low;

                    if (p[0] != '\\' || p[1] != 'u' ||
                        json_decode_hex4(p + 2, &low) != 0 ||
                        low < 0xdc00u || low > 0xdfffu) {
                        free(out);
                        return NULL;
                    }
                    p += 6;
                    codepoint = 0x10000u + (((codepoint - 0xd800u) << 10) |
                        (low - 0xdc00u));
                }
                if (json_append_utf8(out, cap, &len, codepoint) != 0) {
                    free(out);
                    return NULL;
                }
                break;
            }
            case '\0':
                out[len] = '\0';
                if (end_out != NULL) *end_out = p;
                return out;
            default:
                free(out);
                return NULL;
            }
        } else {
            out[len++] = *p++;
        }
    }
    out[len] = '\0';
    if (end_out != NULL) {
        *end_out = p;
    }
    return out;
}

static const char *json_find_object_value(const char *json, const char *field) {
    const char *p = json;

    json_skip_ws(&p);
    if (*p != '{') return NULL;
    p++;
    json_skip_ws(&p);
    if (*p == '}') return NULL;
    for (;;) {
        const char *end = NULL;
        char *key;
        int matched;

        if (*p != '"') return NULL;
        key = decode_json_string(p + 1, &end);
        if (key == NULL || end == NULL || *end != '"') {
            free(key);
            return NULL;
        }
        p = end + 1;
        json_skip_ws(&p);
        if (*p != ':') {
            free(key);
            return NULL;
        }
        p++;
        json_skip_ws(&p);
        matched = strcmp(key, field) == 0;
        free(key);
        if (matched) {
            return p;
        }
        if (!json_parse_value(&p)) {
            return NULL;
        }
        json_skip_ws(&p);
        if (*p == '}') {
            return NULL;
        }
        if (*p != ',') {
            return NULL;
        }
        p++;
        json_skip_ws(&p);
    }
}

static const char *json_find_params_value(const char *json, const char *field) {
    const char *params = json_find_object_value(json, "params");

    if (params == NULL) {
        return NULL;
    }
    return json_find_object_value(params, field);
}

static char *json_extract_str(const char *json, const char *field) {
    const char *p = json_find_object_value(json, field);

    if (p == NULL || *p != '"') {
        return NULL;
    }
    return decode_json_string(p + 1, NULL);
}

static char *json_extract_param_str(const char *json, const char *field) {
    const char *p = json_find_params_value(json, field);

    if (p == NULL || *p != '"') {
        return NULL;
    }
    return decode_json_string(p + 1, NULL);
}

static int json_parse_string_value(const char **p) {
    if (**p != '"') return 0;
    (*p)++;
    while (**p != '\0') {
        unsigned char c = (unsigned char)**p;

        if (c == '"') {
            (*p)++;
            return 1;
        }
        if (c < 0x20) return 0;
        if (c == '\\') {
            (*p)++;
            switch (**p) {
            case '"':
            case '\\':
            case '/':
            case 'b':
            case 'f':
            case 'n':
            case 'r':
            case 't':
                (*p)++;
                break;
            case 'u':
                (*p)++;
                for (int i = 0; i < 4; i++) {
                    char h = **p;
                    if (!((h >= '0' && h <= '9') || (h >= 'a' && h <= 'f') ||
                          (h >= 'A' && h <= 'F'))) {
                        return 0;
                    }
                    (*p)++;
                }
                break;
            default:
                return 0;
            }
        } else {
            (*p)++;
        }
    }
    return 0;
}

static int json_parse_literal(const char **p, const char *literal) {
    size_t n = strlen(literal);

    if (strncmp(*p, literal, n) != 0) return 0;
    *p += n;
    return 1;
}

static int json_parse_number(const char **p) {
    const char *s = *p;

    if (*s == '-') s++;
    if (*s == '0') {
        s++;
    } else if (*s >= '1' && *s <= '9') {
        do {
            s++;
        } while (*s >= '0' && *s <= '9');
    } else {
        return 0;
    }
    if (*s == '.') {
        s++;
        if (!(*s >= '0' && *s <= '9')) return 0;
        do {
            s++;
        } while (*s >= '0' && *s <= '9');
    }
    if (*s == 'e' || *s == 'E') {
        s++;
        if (*s == '+' || *s == '-') s++;
        if (!(*s >= '0' && *s <= '9')) return 0;
        do {
            s++;
        } while (*s >= '0' && *s <= '9');
    }
    *p = s;
    return 1;
}

static int json_parse_array(const char **p) {
    if (**p != '[') return 0;
    (*p)++;
    json_skip_ws(p);
    if (**p == ']') {
        (*p)++;
        return 1;
    }
    for (;;) {
        if (!json_parse_value(p)) return 0;
        json_skip_ws(p);
        if (**p == ']') {
            (*p)++;
            return 1;
        }
        if (**p != ',') return 0;
        (*p)++;
        json_skip_ws(p);
        if (**p == ']') return 0;
    }
}

static int json_parse_object(const char **p) {
    if (**p != '{') return 0;
    (*p)++;
    json_skip_ws(p);
    if (**p == '}') {
        (*p)++;
        return 1;
    }
    for (;;) {
        if (!json_parse_string_value(p)) return 0;
        json_skip_ws(p);
        if (**p != ':') return 0;
        (*p)++;
        json_skip_ws(p);
        if (!json_parse_value(p)) return 0;
        json_skip_ws(p);
        if (**p == '}') {
            (*p)++;
            return 1;
        }
        if (**p != ',') return 0;
        (*p)++;
        json_skip_ws(p);
        if (**p == '}') return 0;
    }
}

static int json_parse_value(const char **p) {
    json_skip_ws(p);
    switch (**p) {
    case '{':
        return json_parse_object(p);
    case '[':
        return json_parse_array(p);
    case '"':
        return json_parse_string_value(p);
    case 't':
        return json_parse_literal(p, "true");
    case 'f':
        return json_parse_literal(p, "false");
    case 'n':
        return json_parse_literal(p, "null");
    default:
        if (**p == '-' || (**p >= '0' && **p <= '9')) {
            return json_parse_number(p);
        }
        return 0;
    }
}

static int validate_json_request(const char *json) {
    const char *p = json;

    json_skip_ws(&p);
    if (!json_parse_object(&p)) {
        return 0;
    }
    json_skip_ws(&p);
    return *p == '\0';
}

static char *json_extract_id(const char *json) {
    const char *p = json_find_object_value(json, "id");
    Buf b;
    char *out;

    if (p == NULL || *p == '\0') {
        return copy_string("null");
    }
    buf_init(&b);
    if (b.data == NULL) return NULL;
    if (*p == '"') {
        char *decoded = decode_json_string(p + 1, NULL);

        if (decoded == NULL) {
            buf_free(&b);
            return NULL;
        }
        if (buf_append_json_string(&b, decoded) != 0) {
            free(decoded);
            buf_free(&b);
            return NULL;
        }
        free(decoded);
    } else {
        while (*p != '\0' && *p != ',' && *p != '}') {
            if (buf_append_char(&b, *p) != 0) {
                buf_free(&b);
                return NULL;
            }
            p++;
        }
        while (b.len > 0 &&
               (b.data[b.len - 1] == ' ' || b.data[b.len - 1] == '\t' ||
                b.data[b.len - 1] == '\r' || b.data[b.len - 1] == '\n')) {
            b.data[--b.len] = '\0';
        }
    }
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static int string_array_push_take(StringArray *arr, char *item) {
    char **next = realloc(arr->items, sizeof(*arr->items) * (arr->len + 1));

    if (next == NULL) {
        free(item);
        return -1;
    }
    arr->items = next;
    arr->items[arr->len++] = item;
    return 0;
}

static int string_array_push_copy(StringArray *arr, const char *item) {
    return string_array_push_take(arr, copy_string(item));
}

static int json_extract_str_array_at(const char *p, StringArray *out) {
    memset(out, 0, sizeof(*out));
    if (p == NULL) {
        return 0;
    }
    if (*p != '[') {
        return -1;
    }
    p++;
    while (*p != '\0') {
        char *item;

        while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n' || *p == ',') {
            p++;
        }
        if (*p == ']') {
            return 0;
        }
        if (*p != '"') {
            string_array_free(out);
            return -1;
        }
        item = decode_json_string(p + 1, &p);
        if (item == NULL || *p != '"') {
            free(item);
            string_array_free(out);
            return -1;
        }
        p++;
        if (string_array_push_take(out, item) != 0) {
            string_array_free(out);
            return -1;
        }
    }
    string_array_free(out);
    return -1;
}

static int json_extract_param_str_array(const char *json, const char *field, StringArray *out) {
    return json_extract_str_array_at(json_find_params_value(json, field), out);
}

static char *json_extract_raw(const char *json, const char *field);

static void string_array_free(StringArray *arr) {
    if (arr == NULL) return;
    for (size_t i = 0; i < arr->len; i++) {
        free(arr->items[i]);
    }
    free(arr->items);
    arr->items = NULL;
    arr->len = 0;
}

static void binding_template_free(BindingTemplate *binding) {
    if (binding == NULL) return;
    free(binding->template_cid);
    free(binding->concept_name_json);
    free(binding->library_tag_json);
    free(binding->family_json);
    free(binding->contract_cid_json);
    memset(binding, 0, sizeof(*binding));
}

static void binding_list_free(BindingList *list) {
    if (list == NULL) return;
    for (size_t i = 0; i < list->len; i++) {
        binding_template_free(&list->items[i]);
    }
    free(list->items);
    list->items = NULL;
    list->len = 0;
}

static int binding_list_push_take(BindingList *list, BindingTemplate *binding) {
    BindingTemplate *next = realloc(list->items, sizeof(*list->items) * (list->len + 1));

    if (next == NULL) {
        binding_template_free(binding);
        return -1;
    }
    list->items = next;
    list->items[list->len++] = *binding;
    memset(binding, 0, sizeof(*binding));
    return 0;
}

static int parse_binding_templates_at(const char *p, BindingList *out) {
    memset(out, 0, sizeof(*out));
    if (p == NULL) {
        return 0;
    }
    json_skip_ws(&p);
    if (*p != '[') {
        return -1;
    }
    p++;
    json_skip_ws(&p);
    if (*p == ']') {
        return 0;
    }
    for (;;) {
        const char *start;
        const char *end;
        char *raw;
        BindingTemplate binding;

        memset(&binding, 0, sizeof(binding));
        json_skip_ws(&p);
        start = p;
        if (!json_parse_value(&p)) {
            binding_list_free(out);
            return -1;
        }
        end = p;
        if (*start == '{') {
            raw = copy_n(start, (size_t)(end - start));
            if (raw == NULL) {
                binding_list_free(out);
                return -1;
            }
            binding.template_cid = json_extract_str(raw, "template_cid");
            if (binding.template_cid != NULL && binding.template_cid[0] != '\0') {
                binding.concept_name_json = json_extract_raw(raw, "concept_name");
                binding.library_tag_json = json_extract_raw(raw, "library_tag");
                binding.family_json = json_extract_raw(raw, "family");
                binding.contract_cid_json = json_extract_raw(raw, "contract_cid");
                if (binding.concept_name_json == NULL) binding.concept_name_json = copy_string("null");
                if (binding.library_tag_json == NULL) binding.library_tag_json = copy_string("null");
                if (binding.family_json == NULL) binding.family_json = copy_string("null");
                if (binding.contract_cid_json == NULL) binding.contract_cid_json = copy_string("null");
                if (binding.concept_name_json == NULL || binding.library_tag_json == NULL ||
                    binding.family_json == NULL || binding.contract_cid_json == NULL ||
                    binding_list_push_take(out, &binding) != 0) {
                    free(raw);
                    binding_template_free(&binding);
                    binding_list_free(out);
                    return -1;
                }
            }
            binding_template_free(&binding);
            free(raw);
        }
        json_skip_ws(&p);
        if (*p == ']') {
            return 0;
        }
        if (*p != ',') {
            binding_list_free(out);
            return -1;
        }
        p++;
    }
}

static int json_extract_param_binding_templates(const char *json, BindingList *out) {
    return parse_binding_templates_at(json_find_params_value(json, "binding_templates"), out);
}

static const BindingTemplate *binding_for_template_cid(const BindingList *list, const char *cid) {
    if (list == NULL || cid == NULL) return NULL;
    for (size_t i = 0; i < list->len; i++) {
        if (list->items[i].template_cid != NULL &&
            strcmp(list->items[i].template_cid, cid) == 0) {
            return &list->items[i];
        }
    }
    return NULL;
}

static int has_suffix(const char *s, const char *suffix) {
    size_t sl = strlen(s == NULL ? "" : s);
    size_t tl = strlen(suffix == NULL ? "" : suffix);

    return sl >= tl && strcmp(s + sl - tl, suffix) == 0;
}

static char *join_path(const char *a, const char *b) {
    size_t al = strlen(a == NULL ? "" : a);
    size_t bl = strlen(b == NULL ? "" : b);
    int needs_slash = al > 0 && a[al - 1] != '/';
    char *out = malloc(al + (needs_slash ? 1u : 0u) + bl + 1u);
    size_t pos = al;

    if (out == NULL) return NULL;
    memcpy(out, a == NULL ? "" : a, al);
    if (needs_slash) {
        out[pos++] = '/';
    }
    memcpy(out + pos, b == NULL ? "" : b, bl);
    out[pos + bl] = '\0';
    return out;
}

static char *resolve_source_path(const char *workspace, const char *source_path) {
    if (source_path == NULL || source_path[0] == '\0' || strcmp(source_path, ".") == 0) {
        return copy_string(workspace);
    }
    if (source_path[0] == '/') {
        return copy_string(source_path);
    }
    return join_path(workspace, source_path);
}

static void normalize_slashes(char *path) {
    if (path == NULL) return;
    for (char *p = path; *p != '\0'; p++) {
        if (*p == '\\') {
            *p = '/';
        }
    }
}

static char *relative_path(const char *workspace, const char *path) {
    size_t wl = strlen(workspace == NULL ? "" : workspace);
    const char *rel = path == NULL ? "" : path;

    while (wl > 1 && workspace[wl - 1] == '/') {
        wl--;
    }
    if (workspace != NULL && strncmp(path, workspace, wl) == 0 &&
        (path[wl] == '/' || path[wl] == '\0')) {
        rel = path + wl;
        if (*rel == '/') rel++;
    }
    if (strncmp(rel, "./", 2) == 0) {
        rel += 2;
    }
    char *out = copy_string(*rel == '\0' ? "." : rel);
    normalize_slashes(out);
    return out;
}

static char *read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    long len;
    char *data;

    if (f == NULL) return NULL;
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    len = ftell(f);
    if (len < 0 || fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }
    data = malloc((size_t)len + 1u);
    if (data == NULL) {
        fclose(f);
        return NULL;
    }
    if (fread(data, 1, (size_t)len, f) != (size_t)len) {
        free(data);
        fclose(f);
        return NULL;
    }
    data[len] = '\0';
    fclose(f);
    return data;
}

static void acc_init(LiftAccumulator *acc) {
    memset(acc, 0, sizeof(*acc));
    buf_init(&acc->ir);
    buf_init(&acc->concept_citations);
    buf_init(&acc->diagnostics);
}

static void acc_free(LiftAccumulator *acc) {
    buf_free(&acc->ir);
    buf_free(&acc->concept_citations);
    buf_free(&acc->diagnostics);
}

static int acc_append_json_item(Buf *buf, size_t *count, const char *json) {
    if (*count > 0 && buf_append_char(buf, ',') != 0) return -1;
    if (buf_append(buf, json) != 0) return -1;
    (*count)++;
    return 0;
}

static int acc_add_diagnostic(LiftAccumulator *acc, const char *kind, const char *path, const char *message) {
    Buf b;
    int ok;

    buf_init(&b);
    if (b.data == NULL) return -1;
    ok = buf_append(&b, "{\"kind\":") == 0 &&
        buf_append_json_string(&b, kind) == 0 &&
        buf_append(&b, ",\"message\":") == 0 &&
        buf_append_json_string(&b, message) == 0 &&
        buf_append(&b, ",\"path\":") == 0 &&
        buf_append_json_string(&b, path) == 0 &&
        buf_append_char(&b, '}') == 0;
    if (!ok || acc_append_json_item(&acc->diagnostics, &acc->diagnostic_count, b.data) != 0) {
        buf_free(&b);
        return -1;
    }
    buf_free(&b);
    return 0;
}

static char *cid_for_bytes(const char *data, size_t len) {
    uint8_t out[64];
    blake3_hasher hasher;
    const char prefix[] = "blake3-512:";
    char *cid = malloc(strlen(prefix) + sizeof(out) * 2 + 1);

    if (cid == NULL) return NULL;
    blake3_hasher_init(&hasher);
    blake3_hasher_update(&hasher, data == NULL ? "" : data, len);
    blake3_hasher_finalize(&hasher, out, sizeof(out));
    memcpy(cid, prefix, strlen(prefix));
    for (size_t i = 0; i < sizeof(out); i++) {
        static const char hex[] = "0123456789abcdef";

        cid[strlen(prefix) + i * 2] = hex[out[i] >> 4];
        cid[strlen(prefix) + i * 2 + 1] = hex[out[i] & 0x0f];
    }
    cid[strlen(prefix) + sizeof(out) * 2] = '\0';
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

static char *json_extract_raw(const char *json, const char *field) {
    const char *p = json_find_object_value(json, field);
    const char *start;

    if (p == NULL) return NULL;
    start = p;
    if (!json_parse_value(&p)) return NULL;
    while (start < p && isspace((unsigned char)*start)) start++;
    while (p > start && isspace((unsigned char)p[-1])) p--;
    return copy_n(start, (size_t)(p - start));
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

static int append_optional_string_field(Buf *out, int *first, const char *key, const char *value) {
    return value == NULL ? 0 : append_string_field(out, first, key, value);
}

typedef struct {
    char *args_jcs;
    char *args_jcs_cid;
    char *artifact_kind;
    char *callsite_cid;
    char *concept_cid;
    char *concept_name;
    char *concept_site_cid;
    char *emitted_kit_cid;
    char *emitted_kit_id;
    char *emitted_kit_kind;
    char *emitted_target_language;
    char *emitted_target_library_tag;
    char *loss_record_cid;
    char *operation_kind;
    char *policy_cid;
    char *schema_version;
    char *shape_cid;
    char *sugar_dict_cid;
    char *term_position;
} ConceptPayload;

static void concept_payload_free(ConceptPayload *payload) {
    if (payload == NULL) return;
    free(payload->args_jcs);
    free(payload->args_jcs_cid);
    free(payload->artifact_kind);
    free(payload->callsite_cid);
    free(payload->concept_cid);
    free(payload->concept_name);
    free(payload->concept_site_cid);
    free(payload->emitted_kit_cid);
    free(payload->emitted_kit_id);
    free(payload->emitted_kit_kind);
    free(payload->emitted_target_language);
    free(payload->emitted_target_library_tag);
    free(payload->loss_record_cid);
    free(payload->operation_kind);
    free(payload->policy_cid);
    free(payload->schema_version);
    free(payload->shape_cid);
    free(payload->sugar_dict_cid);
    free(payload->term_position);
    memset(payload, 0, sizeof(*payload));
}

static int parse_concept_payload(const char *raw, ConceptPayload *payload) {
    const char *emitted_by;

    memset(payload, 0, sizeof(*payload));
    payload->args_jcs = json_extract_raw(raw, "args_jcs");
    payload->args_jcs_cid = json_extract_str(raw, "args_jcs_cid");
    payload->artifact_kind = json_extract_str(raw, "artifact_kind");
    payload->callsite_cid = json_extract_str(raw, "callsite_cid");
    payload->concept_cid = json_extract_str(raw, "concept_cid");
    payload->concept_name = json_extract_str(raw, "concept_name");
    payload->concept_site_cid = json_extract_str(raw, "concept_site_cid");
    payload->loss_record_cid = json_extract_str(raw, "loss_record_cid");
    payload->operation_kind = json_extract_str(raw, "operation_kind");
    payload->policy_cid = json_extract_str(raw, "policy_cid");
    payload->schema_version = json_extract_str(raw, "schema_version");
    payload->shape_cid = json_extract_str(raw, "shape_cid");
    payload->sugar_dict_cid = json_extract_str(raw, "sugar_dict_cid");
    payload->term_position = json_extract_raw(raw, "term_position");

    emitted_by = json_find_object_value(raw, "emitted_by");
    if (emitted_by != NULL && *emitted_by == '{') {
        payload->emitted_kit_cid = json_extract_str(emitted_by, "kit_cid");
        payload->emitted_kit_id = json_extract_str(emitted_by, "kit_id");
        payload->emitted_kit_kind = json_extract_str(emitted_by, "kit_kind");
        payload->emitted_target_language = json_extract_str(emitted_by, "target_language");
        payload->emitted_target_library_tag = json_extract_str(emitted_by, "target_library_tag");
    }
    return 0;
}

static int is_uint_array_json(const char *s) {
    const char *p = s;

    if (p == NULL) return 0;
    json_skip_ws(&p);
    if (*p != '[') return 0;
    p++;
    json_skip_ws(&p);
    if (*p == ']') return 1;
    for (;;) {
        if (!isdigit((unsigned char)*p)) return 0;
        if (*p == '0') {
            p++;
        } else {
            while (isdigit((unsigned char)*p)) p++;
        }
        json_skip_ws(&p);
        if (*p == ']') return 1;
        if (*p != ',') return 0;
        p++;
        json_skip_ws(&p);
        if (*p == ']') return 0;
    }
}

static int concept_payload_has_required_shape(const ConceptPayload *p) {
    if (p->args_jcs_cid == NULL || p->concept_cid == NULL ||
        p->concept_site_cid == NULL || p->emitted_kit_cid == NULL ||
        p->emitted_kit_id == NULL || p->emitted_kit_kind == NULL ||
        p->emitted_target_language == NULL || p->emitted_target_library_tag == NULL ||
        p->loss_record_cid == NULL || p->operation_kind == NULL ||
        p->schema_version == NULL || p->shape_cid == NULL ||
        p->sugar_dict_cid == NULL || p->term_position == NULL) {
        return 0;
    }
    if (strcmp(p->emitted_kit_kind, "realize") != 0) return 0;
    if (strcmp(p->emitted_target_language, "c") != 0) return 0;
    if (p->args_jcs != NULL && p->args_jcs[0] != '[') return 0;
    return is_uint_array_json(p->term_position);
}

static int concept_payload_has_valid_cids(const ConceptPayload *p) {
    return is_valid_blake3_512_cid(p->args_jcs_cid) &&
        (p->callsite_cid == NULL || is_valid_blake3_512_cid(p->callsite_cid)) &&
        is_valid_blake3_512_cid(p->concept_cid) &&
        is_valid_blake3_512_cid(p->concept_site_cid) &&
        is_valid_blake3_512_cid(p->emitted_kit_cid) &&
        is_valid_blake3_512_cid(p->loss_record_cid) &&
        (p->policy_cid == NULL || is_valid_blake3_512_cid(p->policy_cid)) &&
        is_valid_blake3_512_cid(p->shape_cid) &&
        is_valid_blake3_512_cid(p->sugar_dict_cid);
}

static char *canonical_concept_payload_json(const ConceptPayload *p) {
    Buf out;
    int first = 1;
    int emitted_first = 1;
    char *json;

    buf_init(&out);
    if (out.data == NULL) return NULL;
    if (buf_append_char(&out, '{') != 0 ||
        (p->args_jcs != NULL && append_raw_field(&out, &first, "args_jcs", p->args_jcs) != 0) ||
        append_string_field(&out, &first, "args_jcs_cid", p->args_jcs_cid) != 0 ||
        append_string_field(&out, &first, "artifact_kind", CONCEPT_CITATION_COMMENT_KIND) != 0 ||
        append_optional_string_field(&out, &first, "callsite_cid", p->callsite_cid) != 0 ||
        append_string_field(&out, &first, "concept_cid", p->concept_cid) != 0 ||
        append_optional_string_field(&out, &first, "concept_name", p->concept_name) != 0 ||
        append_string_field(&out, &first, "concept_site_cid", p->concept_site_cid) != 0 ||
        append_field_key(&out, &first, "emitted_by") != 0 ||
        buf_append_char(&out, '{') != 0 ||
        append_string_field(&out, &emitted_first, "kit_cid", p->emitted_kit_cid) != 0 ||
        append_string_field(&out, &emitted_first, "kit_id", p->emitted_kit_id) != 0 ||
        append_string_field(&out, &emitted_first, "kit_kind", p->emitted_kit_kind) != 0 ||
        append_string_field(&out, &emitted_first, "target_language", p->emitted_target_language) != 0 ||
        append_string_field(&out, &emitted_first, "target_library_tag", p->emitted_target_library_tag) != 0 ||
        buf_append_char(&out, '}') != 0 ||
        append_string_field(&out, &first, "loss_record_cid", p->loss_record_cid) != 0 ||
        append_string_field(&out, &first, "operation_kind", p->operation_kind) != 0 ||
        append_optional_string_field(&out, &first, "policy_cid", p->policy_cid) != 0 ||
        append_string_field(&out, &first, "schema_version", p->schema_version) != 0 ||
        append_string_field(&out, &first, "shape_cid", p->shape_cid) != 0 ||
        append_string_field(&out, &first, "sugar_dict_cid", p->sugar_dict_cid) != 0 ||
        append_raw_field(&out, &first, "term_position", p->term_position) != 0 ||
        buf_append_char(&out, '}') != 0) {
        buf_free(&out);
        return NULL;
    }
    json = buf_take(&out);
    buf_free(&out);
    return json;
}

static int readable_file(const char *path) {
    struct stat st;

    return path != NULL && stat(path, &st) == 0 && S_ISREG(st.st_mode);
}

static char *find_file_from_base(const char *base, const char *rel) {
    char *cursor = copy_string(base);

    if (cursor == NULL) return NULL;
    while (cursor[0] != '\0') {
        char *candidate = join_path(cursor, rel);
        char *slash;

        if (candidate != NULL && readable_file(candidate)) {
            free(cursor);
            return candidate;
        }
        free(candidate);
        slash = strrchr(cursor, '/');
        if (slash == NULL) break;
        if (slash == cursor) {
            cursor[1] = '\0';
            candidate = join_path(cursor, rel);
            if (candidate != NULL && readable_file(candidate)) {
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

static char *find_catalog_index_path(const char *workspace) {
    const char rel[] = "menagerie/concept-shapes/catalog/index.json";
    const char *env_root = getenv("PROVEKIT_REPO_ROOT");
    char cwd[PATH_MAX];
    char *candidate;

    if (env_root != NULL && env_root[0] != '\0') {
        candidate = join_path(env_root, rel);
        if (candidate != NULL && readable_file(candidate)) return candidate;
        free(candidate);
    }
    if (workspace != NULL && workspace[0] != '\0') {
        candidate = join_path(workspace, rel);
        if (candidate != NULL && readable_file(candidate)) return candidate;
        free(candidate);
    }
    if (getcwd(cwd, sizeof(cwd)) != NULL) {
        candidate = find_file_from_base(cwd, rel);
        if (candidate != NULL) return candidate;
    }
    return NULL;
}

static int catalog_lookup_concept(
    const char *workspace,
    const char *concept_cid,
    int *catalog_available,
    int *found,
    char **shape_cid,
    char **operation_kind
) {
    char *index_path = find_catalog_index_path(workspace);
    char *raw;
    char *hit;
    char *name_key;
    char *object_end;
    char *name = NULL;

    *catalog_available = 0;
    *found = 0;
    *shape_cid = NULL;
    *operation_kind = NULL;
    if (index_path == NULL) return 0;
    raw = read_file(index_path);
    free(index_path);
    if (raw == NULL) return 0;
    *catalog_available = 1;
    hit = strstr(raw, concept_cid);
    if (hit == NULL) {
        free(raw);
        return 0;
    }
    object_end = strchr(hit, '}');
    name_key = strstr(hit, "\"name\"");
    if (name_key != NULL && object_end != NULL && name_key < object_end) {
        char *colon = strchr(name_key, ':');
        if (colon != NULL) {
            const char *p = colon + 1;
            json_skip_ws(&p);
            if (*p == '"') name = decode_json_string(p + 1, NULL);
        }
    }
    if (name == NULL) {
        free(raw);
        return 0;
    }
    *shape_cid = copy_string(concept_cid);
    if (strncmp(name, "concept:", 8) == 0) {
        *operation_kind = copy_string(name + 8);
    } else {
        *operation_kind = copy_string(name);
    }
    if (*shape_cid == NULL || *operation_kind == NULL) {
        free(name);
        free(raw);
        free(*shape_cid);
        free(*operation_kind);
        *shape_cid = NULL;
        *operation_kind = NULL;
        return -1;
    }
    *found = 1;
    free(name);
    free(raw);
    return 0;
}

static int acc_add_concept_diag(
    LiftAccumulator *acc,
    const char *kind,
    const char *path,
    unsigned line,
    const char *detail
) {
    char message[256];

    (void)snprintf(message, sizeof(message), "line %u: %s", line, detail == NULL ? kind : detail);
    return acc_add_diagnostic(acc, kind, path, message);
}

static int acc_add_concept_citation(
    LiftAccumulator *acc,
    const char *rel,
    unsigned line,
    const ConceptPayload *p,
    const char *payload_cid
) {
    Buf out;
    int first = 1;
    int ok;

    buf_init(&out);
    if (out.data == NULL) return -1;
    ok = buf_append_char(&out, '{') == 0 &&
        (p->args_jcs != NULL ? append_raw_field(&out, &first, "args_jcs", p->args_jcs) == 0 : 1) &&
        append_string_field(&out, &first, "args_jcs_cid", p->args_jcs_cid) == 0 &&
        append_optional_string_field(&out, &first, "callsite_cid", p->callsite_cid) == 0 &&
        append_string_field(&out, &first, "concept_cid", p->concept_cid) == 0 &&
        append_optional_string_field(&out, &first, "concept_name", p->concept_name) == 0 &&
        append_string_field(&out, &first, "concept_site_cid", p->concept_site_cid) == 0 &&
        append_string_field(&out, &first, "file", rel) == 0 &&
        append_string_field(&out, &first, "kind", CONCEPT_CITATION_COMMENT_KIND) == 0 &&
        append_string_field(&out, &first, "loss_record_cid", p->loss_record_cid) == 0 &&
        append_field_key(&out, &first, "line") == 0 &&
        buf_append_uint(&out, line) == 0 &&
        append_string_field(&out, &first, "operation_kind", p->operation_kind) == 0 &&
        append_string_field(&out, &first, "payload_cid", payload_cid) == 0 &&
        append_optional_string_field(&out, &first, "policy_cid", p->policy_cid) == 0 &&
        append_string_field(&out, &first, "shape_cid", p->shape_cid) == 0 &&
        append_string_field(&out, &first, "source_kind", "native-surface") == 0 &&
        append_string_field(&out, &first, "sugar_dict_cid", p->sugar_dict_cid) == 0 &&
        append_raw_field(&out, &first, "term_position", p->term_position) == 0 &&
        buf_append_char(&out, '}') == 0;
    if (!ok ||
        acc_append_json_item(&acc->concept_citations, &acc->concept_citation_count, out.data) != 0) {
        buf_free(&out);
        return -1;
    }
    buf_free(&out);
    return 0;
}

static int validate_concept_citation(
    LiftAccumulator *acc,
    const char *workspace,
    const char *rel,
    unsigned line,
    const char *raw_payload,
    const char *emitted_payload_cid
) {
    ConceptPayload payload;
    char *canonical_payload = NULL;
    char *computed_payload_cid = NULL;
    char *computed_args_cid = NULL;
    char *catalog_shape_cid = NULL;
    char *catalog_operation_kind = NULL;
    int catalog_available = 0;
    int catalog_found = 0;
    int rc = 0;

    memset(&payload, 0, sizeof(payload));
    if (!validate_json_request(raw_payload)) {
        return acc_add_concept_diag(acc, "concept-citation:malformed-json", rel, line, "malformed JSON");
    }
    if (parse_concept_payload(raw_payload, &payload) != 0) {
        return -1;
    }
    if (payload.schema_version == NULL || strcmp(payload.schema_version, "1") != 0) {
        rc = acc_add_concept_diag(acc, "concept-citation:unknown-schema-version", rel, line, "unknown schema_version");
        goto done;
    }
    if (!concept_payload_has_required_shape(&payload) ||
        payload.artifact_kind == NULL ||
        strcmp(payload.artifact_kind, CONCEPT_CITATION_COMMENT_KIND) != 0) {
        rc = acc_add_concept_diag(acc, "concept-citation:malformed-json", rel, line, "malformed concept-citation payload");
        goto done;
    }
    if (!concept_payload_has_valid_cids(&payload) ||
        !is_valid_blake3_512_cid(emitted_payload_cid)) {
        rc = acc_add_concept_diag(acc, "concept-citation:malformed-cid", rel, line, "malformed CID");
        goto done;
    }

    canonical_payload = canonical_concept_payload_json(&payload);
    if (canonical_payload == NULL) {
        rc = -1;
        goto done;
    }
    computed_payload_cid = cid_for_bytes(canonical_payload, strlen(canonical_payload));
    if (computed_payload_cid == NULL) {
        rc = -1;
        goto done;
    }
    if (strcmp(computed_payload_cid, emitted_payload_cid) != 0) {
        rc = acc_add_concept_diag(acc, "concept-citation:payload-cid-mismatch", rel, line, "payload CID mismatch");
        goto done;
    }
    if (payload.args_jcs != NULL) {
        computed_args_cid = cid_for_bytes(payload.args_jcs, strlen(payload.args_jcs));
        if (computed_args_cid == NULL) {
            rc = -1;
            goto done;
        }
        if (strcmp(computed_args_cid, payload.args_jcs_cid) != 0) {
            rc = acc_add_concept_diag(acc, "concept-citation:args-cid-mismatch", rel, line, "args CID mismatch");
            goto done;
        }
    }
    if (catalog_lookup_concept(
            workspace,
            payload.concept_cid,
            &catalog_available,
            &catalog_found,
            &catalog_shape_cid,
            &catalog_operation_kind) != 0) {
        rc = -1;
        goto done;
    }
    if (catalog_available && !catalog_found) {
        rc = acc_add_concept_diag(acc, "concept-citation:concept-not-in-catalog", rel, line, "concept CID not in catalog");
        goto done;
    }
    if (catalog_found && strcmp(catalog_shape_cid, payload.shape_cid) != 0) {
        rc = acc_add_concept_diag(acc, "concept-citation:shape-mismatch", rel, line, "shape CID mismatch");
        goto done;
    }
    if (catalog_found && strcmp(catalog_operation_kind, payload.operation_kind) != 0) {
        rc = acc_add_concept_diag(acc, "concept-citation:operation-kind-mismatch", rel, line, "operation_kind mismatch");
        goto done;
    }
    rc = acc_add_concept_citation(acc, rel, line, &payload, computed_payload_cid);

done:
    concept_payload_free(&payload);
    free(canonical_payload);
    free(computed_payload_cid);
    free(computed_args_cid);
    free(catalog_shape_cid);
    free(catalog_operation_kind);
    return rc;
}

static char *trim_line_segment(const char *start, const char *end) {
    while (start < end && isspace((unsigned char)*start)) start++;
    while (end > start && isspace((unsigned char)end[-1])) end--;
    return copy_n(start, (size_t)(end - start));
}

static int scan_concept_citations(
    LiftAccumulator *acc,
    const char *workspace,
    const char *rel,
    const char *source
) {
    const char payload_prefix[] = "// provekit-concept: ";
    const char cid_prefix[] = "// provekit-concept-payload-cid: ";
    const char *p = source == NULL ? "" : source;
    unsigned line_no = 1;

    while (*p != '\0') {
        const char *line_start = p;
        const char *line_end;
        const char *next_start;
        char *line;
        int consumed_next = 0;
        int rc;

        while (*p != '\0' && *p != '\n') p++;
        line_end = p;
        if (line_end > line_start && line_end[-1] == '\r') line_end--;
        next_start = *p == '\n' ? p + 1 : p;
        line = trim_line_segment(line_start, line_end);
        if (line == NULL) return -1;
        if (strncmp(line, payload_prefix, strlen(payload_prefix)) == 0) {
            char *next_line = NULL;
            char *payload_cid = NULL;
            const char *next_end = next_start;

            while (*next_end != '\0' && *next_end != '\n') next_end++;
            if (next_end > next_start && next_end[-1] == '\r') next_end--;
            next_line = trim_line_segment(next_start, next_end);
            if (next_line == NULL) {
                free(line);
                return -1;
            }
            if (strncmp(next_line, cid_prefix, strlen(cid_prefix)) == 0) {
                payload_cid = copy_string(next_line + strlen(cid_prefix));
                consumed_next = 1;
            }
            if (payload_cid == NULL) {
                rc = acc_add_concept_diag(acc, "concept-citation:malformed-cid", rel, line_no, "missing payload CID line");
            } else {
                rc = validate_concept_citation(
                    acc,
                    workspace,
                    rel,
                    line_no,
                    line + strlen(payload_prefix),
                    payload_cid);
            }
            free(payload_cid);
            free(next_line);
            free(line);
            if (rc != 0) return rc;
            if (consumed_next) {
                p = *next_end == '\n' ? next_end + 1 : next_end;
                line_no += 2;
                continue;
            }
        } else if (strncmp(line, cid_prefix, strlen(cid_prefix)) == 0) {
            rc = acc_add_concept_diag(acc, "concept-citation:orphan-cid-line", rel, line_no, "orphan concept payload CID line");
            free(line);
            if (rc != 0) return rc;
        } else {
            free(line);
        }
        p = next_start;
        line_no++;
    }
    return 0;
}

#ifdef PK_C_ENABLE_CLANG_AST

typedef struct {
    CXCursor *items;
    size_t len;
    size_t cap;
} CursorList;

typedef struct {
    const char *path;
    const char *rel;
    const char *source;
    LiftAccumulator *acc;
    int failed;
} FileLiftCtx;

typedef struct {
    CXCursor cursor;
    unsigned wanted;
    unsigned seen;
    int found;
} NthChildCtx;

typedef struct {
    char *concept;
    char *pre;
    char *post;
} FunctionAnnotations;

static void cursor_list_free(CursorList *list) {
    free(list->items);
    memset(list, 0, sizeof(*list));
}

static int cursor_list_append(CursorList *list, CXCursor cursor) {
    CXCursor *items;
    size_t cap;

    if (list->len == list->cap) {
        cap = list->cap == 0 ? 4 : list->cap * 2;
        if (cap < list->cap) return -1;
        items = realloc(list->items, cap * sizeof(*items));
        if (items == NULL) return -1;
        list->items = items;
        list->cap = cap;
    }
    list->items[list->len++] = cursor;
    return 0;
}

static enum CXChildVisitResult collect_child(CXCursor cursor, CXCursor parent, CXClientData data) {
    CursorList *list = (CursorList *)data;

    (void)parent;
    return cursor_list_append(list, cursor) == 0 ? CXChildVisit_Continue : CXChildVisit_Break;
}

static int cursor_children(CXCursor cursor, CursorList *out) {
    return clang_visitChildren(cursor, collect_child, out) == 0 ? 0 : -1;
}

static enum CXChildVisitResult nth_child_visitor(CXCursor cursor, CXCursor parent, CXClientData data) {
    NthChildCtx *ctx = (NthChildCtx *)data;

    (void)parent;
    if (ctx->seen == ctx->wanted) {
        ctx->cursor = cursor;
        ctx->found = 1;
        return CXChildVisit_Break;
    }
    ctx->seen++;
    return CXChildVisit_Continue;
}

static int get_nth_child(CXCursor cursor, unsigned wanted, CXCursor *out) {
    NthChildCtx ctx;

    memset(&ctx, 0, sizeof(ctx));
    ctx.wanted = wanted;
    ctx.cursor = clang_getNullCursor();
    (void)clang_visitChildren(cursor, nth_child_visitor, &ctx);
    if (!ctx.found) return 0;
    *out = ctx.cursor;
    return 1;
}

static char *cx_string_copy(CXString s) {
    const char *text = clang_getCString(s);
    char *out = copy_string(text == NULL ? "" : text);

    clang_disposeString(s);
    return out;
}

static int collect_param_names(CXCursor function_cursor, StringArray *out) {
    int n = clang_Cursor_getNumArguments(function_cursor);

    memset(out, 0, sizeof(*out));
    if (n < 0) n = 0;
    for (int i = 0; i < n; i++) {
        CXCursor arg = clang_Cursor_getArgument(function_cursor, (unsigned)i);
        char fallback[32];
        char *name = cx_string_copy(clang_getCursorSpelling(arg));

        if (name == NULL) {
            string_array_free(out);
            return -1;
        }
        if (name[0] == '\0') {
            free(name);
            (void)snprintf(fallback, sizeof(fallback), "__arg%d", i);
            name = copy_string(fallback);
            if (name == NULL) {
                string_array_free(out);
                return -1;
            }
        }
        if (string_array_push_take(out, name) != 0) {
            string_array_free(out);
            return -1;
        }
    }
    return 0;
}

static int param_index_for_name(const StringArray *params, const char *name) {
    if (params == NULL || name == NULL || name[0] == '\0') return 0;
    for (size_t i = 0; i < params->len; i++) {
        if (params->items[i] != NULL && strcmp(params->items[i], name) == 0) {
            return (int)i + 1;
        }
    }
    return 0;
}

static int cursor_extent_offsets(CXCursor cursor, unsigned *start_out, unsigned *end_out) {
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXSourceLocation start_loc = clang_getRangeStart(range);
    CXSourceLocation end_loc = clang_getRangeEnd(range);
    unsigned start = 0;
    unsigned end = 0;

    clang_getFileLocation(start_loc, NULL, NULL, NULL, &start);
    clang_getFileLocation(end_loc, NULL, NULL, NULL, &end);
    if (end < start) return -1;
    *start_out = start;
    *end_out = end;
    return 0;
}

static char *trim_copy(const char *s, size_t len);

static char *cursor_source_text(CXCursor cursor, const char *source) {
    unsigned start = 0;
    unsigned end = 0;
    size_t source_len = strlen(source == NULL ? "" : source);

    if (source == NULL || cursor_extent_offsets(cursor, &start, &end) != 0) {
        return copy_string("");
    }
    if ((size_t)start > source_len) start = (unsigned)source_len;
    if ((size_t)end > source_len) end = (unsigned)source_len;
    if (end < start) end = start;
    return trim_copy(source + start, (size_t)(end - start));
}

static char *cursor_kind_name(CXCursor cursor) {
    return cx_string_copy(clang_getCursorKindSpelling(clang_getCursorKind(cursor)));
}

static int append_span_object(Buf *b, CXCursor cursor) {
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXSourceLocation start_loc = clang_getRangeStart(range);
    CXSourceLocation end_loc = clang_getRangeEnd(range);
    unsigned start_line = 1;
    unsigned start_col = 1;
    unsigned end_line = 1;
    unsigned end_col = 1;

    clang_getSpellingLocation(start_loc, NULL, &start_line, &start_col, NULL);
    clang_getSpellingLocation(end_loc, NULL, &end_line, &end_col, NULL);
    if (start_col > 0) start_col--;
    if (end_col > 0) end_col--;
    return buf_append(b, "{\"start_line\":") == 0 &&
        buf_append_uint(b, start_line) == 0 &&
        buf_append(b, ",\"start_col\":") == 0 &&
        buf_append_uint(b, start_col) == 0 &&
        buf_append(b, ",\"end_line\":") == 0 &&
        buf_append_uint(b, end_line) == 0 &&
        buf_append(b, ",\"end_col\":") == 0 &&
        buf_append_uint(b, end_col) == 0 &&
        buf_append_char(b, '}') == 0 ? 0 : -1;
}

static char *template_of_expr(CXCursor cursor, const StringArray *params, const char *source);
static char *template_of_stmt(CXCursor cursor, const StringArray *params, const char *source);
static char *operator_token(CXCursor cursor, int mode_binary);
static enum CXChildVisitResult body_finder(CXCursor cursor, CXCursor parent, CXClientData data);

static char *template_other(CXCursor cursor) {
    char *variant = cursor_kind_name(cursor);
    Buf b;
    char *out;

    if (variant == NULL) return NULL;
    buf_init(&b);
    if (b.data == NULL) {
        free(variant);
        return NULL;
    }
    if (buf_append(&b, "{\"kind\":\"other\",\"variant\":") != 0 ||
        buf_append_json_string(&b, variant) != 0 ||
        buf_append_char(&b, '}') != 0) {
        free(variant);
        buf_free(&b);
        return NULL;
    }
    free(variant);
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static char *template_simple(const char *kind) {
    Buf b;
    char *out;

    buf_init(&b);
    if (b.data == NULL) return NULL;
    if (buf_append(&b, "{\"kind\":") != 0 ||
        buf_append_json_string(&b, kind) != 0 ||
        buf_append_char(&b, '}') != 0) {
        buf_free(&b);
        return NULL;
    }
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static char *template_decl_ref(CXCursor cursor, const StringArray *params) {
    char *name = cx_string_copy(clang_getCursorSpelling(cursor));
    int index = param_index_for_name(params, name);
    Buf b;
    char *out;

    if (name == NULL) return NULL;
    buf_init(&b);
    if (b.data == NULL) {
        free(name);
        return NULL;
    }
    if (index > 0) {
        if (buf_append(&b, "{\"index\":") != 0 ||
            buf_append_uint(&b, (unsigned)index) != 0 ||
            buf_append(&b, ",\"kind\":\"param_ref\"}") != 0) {
            free(name);
            buf_free(&b);
            return NULL;
        }
    } else {
        if (buf_append(&b, "{\"kind\":\"ident\",\"name\":") != 0 ||
            buf_append_json_string(&b, name) != 0 ||
            buf_append_char(&b, '}') != 0) {
            free(name);
            buf_free(&b);
            return NULL;
        }
    }
    free(name);
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static char *template_literal(CXCursor cursor, const char *source, const char *kind) {
    char *text = cursor_source_text(cursor, source);
    Buf b;
    char *out;

    if (text == NULL) return NULL;
    buf_init(&b);
    if (b.data == NULL) {
        free(text);
        return NULL;
    }
    if (buf_append(&b, "{\"kind\":\"literal\",\"literal_kind\":") != 0 ||
        buf_append_json_string(&b, kind) != 0 ||
        buf_append(&b, ",\"text\":") != 0 ||
        buf_append_json_string(&b, text) != 0 ||
        buf_append_char(&b, '}') != 0) {
        free(text);
        buf_free(&b);
        return NULL;
    }
    free(text);
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static char *template_unwrap_expr(CXCursor cursor, const StringArray *params, const char *source) {
    CXCursor child;

    if (get_nth_child(cursor, 0, &child)) {
        return template_of_expr(child, params, source);
    }
    return template_other(cursor);
}

static char *template_call(CXCursor cursor, const StringArray *params, const char *source) {
    CursorList children = {0};
    char *func = NULL;
    Buf b;
    char *out = NULL;

    if (cursor_children(cursor, &children) != 0) return template_other(cursor);
    func = children.len > 0 ? template_of_expr(children.items[0], params, source) : template_simple("unknown");
    if (func == NULL) goto done_no_buf;
    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"args\":[") != 0) goto done;
    for (size_t i = 1; i < children.len; i++) {
        char *arg = template_of_expr(children.items[i], params, source);

        if (arg == NULL) goto done;
        if (i > 1 && buf_append_char(&b, ',') != 0) {
            free(arg);
            goto done;
        }
        if (buf_append(&b, arg) != 0) {
            free(arg);
            goto done;
        }
        free(arg);
    }
    if (buf_append(&b, "],\"func\":") != 0 ||
        buf_append(&b, func) != 0 ||
        buf_append(&b, ",\"kind\":\"call\"}") != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(func);
    cursor_list_free(&children);
    return out == NULL ? template_other(cursor) : out;
}

static char *template_binary(CXCursor cursor, const StringArray *params, const char *source) {
    CursorList children = {0};
    char *op = NULL;
    char *left = NULL;
    char *right = NULL;
    Buf b;
    char *out = NULL;

    if (cursor_children(cursor, &children) != 0) return template_other(cursor);
    op = operator_token(cursor, 1);
    left = children.len > 0 ? template_of_expr(children.items[0], params, source) : template_simple("missing");
    right = children.len > 1 ? template_of_expr(children.items[1], params, source) : template_simple("missing");
    if (op == NULL || left == NULL || right == NULL) goto done_no_buf;
    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"kind\":\"binary\",\"left\":") != 0 ||
        buf_append(&b, left) != 0 ||
        buf_append(&b, ",\"op\":") != 0 ||
        buf_append_json_string(&b, op) != 0 ||
        buf_append(&b, ",\"right\":") != 0 ||
        buf_append(&b, right) != 0 ||
        buf_append_char(&b, '}') != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(op);
    free(left);
    free(right);
    cursor_list_free(&children);
    return out == NULL ? template_other(cursor) : out;
}

static char *template_unary(CXCursor cursor, const StringArray *params, const char *source) {
    char *op = operator_token(cursor, 0);
    char *expr = NULL;
    CXCursor child;
    Buf b;
    char *out = NULL;

    if (get_nth_child(cursor, 0, &child)) {
        expr = template_of_expr(child, params, source);
    } else {
        expr = template_simple("missing");
    }
    if (op == NULL || expr == NULL) goto done_no_buf;
    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"expr\":") != 0 ||
        buf_append(&b, expr) != 0 ||
        buf_append(&b, ",\"kind\":\"unary\",\"op\":") != 0 ||
        buf_append_json_string(&b, op) != 0 ||
        buf_append_char(&b, '}') != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(op);
    free(expr);
    return out == NULL ? template_other(cursor) : out;
}

static char *template_array_subscript(CXCursor cursor, const StringArray *params, const char *source) {
    CursorList children = {0};
    char *base = NULL;
    char *index = NULL;
    Buf b;
    char *out = NULL;

    if (cursor_children(cursor, &children) != 0) return template_other(cursor);
    base = children.len > 0 ? template_of_expr(children.items[0], params, source) : template_simple("missing");
    index = children.len > 1 ? template_of_expr(children.items[1], params, source) : template_simple("missing");
    if (base == NULL || index == NULL) goto done_no_buf;
    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"base\":") != 0 ||
        buf_append(&b, base) != 0 ||
        buf_append(&b, ",\"index\":") != 0 ||
        buf_append(&b, index) != 0 ||
        buf_append(&b, ",\"kind\":\"subscript\"}") != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(base);
    free(index);
    cursor_list_free(&children);
    return out == NULL ? template_other(cursor) : out;
}

static char *template_block(CXCursor cursor, const StringArray *params, const char *source) {
    CursorList children = {0};
    Buf b;
    char *out = NULL;

    if (cursor_children(cursor, &children) != 0) return template_other(cursor);
    buf_init(&b);
    if (b.data == NULL) goto done;
    if (buf_append(&b, "{\"kind\":\"block\",\"stmts\":[") != 0) goto done;
    for (size_t i = 0; i < children.len; i++) {
        char *child = template_of_stmt(children.items[i], params, source);

        if (child == NULL) goto done;
        if (i > 0 && buf_append_char(&b, ',') != 0) {
            free(child);
            goto done;
        }
        if (buf_append(&b, child) != 0) {
            free(child);
            goto done;
        }
        free(child);
    }
    if (buf_append(&b, "]}") != 0) goto done;
    out = buf_take(&b);

done:
    cursor_list_free(&children);
    buf_free(&b);
    return out == NULL ? template_other(cursor) : out;
}

static char *template_return(CXCursor cursor, const StringArray *params, const char *source) {
    CXCursor expr_cursor;
    char *expr = NULL;
    Buf b;
    char *out = NULL;

    if (get_nth_child(cursor, 0, &expr_cursor)) {
        expr = template_of_expr(expr_cursor, params, source);
    } else {
        expr = copy_string("null");
    }
    if (expr == NULL) goto done_no_buf;
    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"expr\":") != 0 ||
        buf_append(&b, expr) != 0 ||
        buf_append(&b, ",\"kind\":\"return\"}") != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(expr);
    return out == NULL ? template_other(cursor) : out;
}

static char *template_of_expr(CXCursor cursor, const StringArray *params, const char *source) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
        return template_unwrap_expr(cursor, params, source);
    case CXCursor_DeclRefExpr:
        return template_decl_ref(cursor, params);
    case CXCursor_CallExpr:
        return template_call(cursor, params, source);
    case CXCursor_BinaryOperator:
    case CXCursor_CompoundAssignOperator:
        return template_binary(cursor, params, source);
    case CXCursor_UnaryOperator:
        return template_unary(cursor, params, source);
    case CXCursor_ArraySubscriptExpr:
        return template_array_subscript(cursor, params, source);
    case CXCursor_IntegerLiteral:
        return template_literal(cursor, source, "integer");
    case CXCursor_FloatingLiteral:
        return template_literal(cursor, source, "float");
    case CXCursor_StringLiteral:
        return template_literal(cursor, source, "string");
    case CXCursor_CharacterLiteral:
        return template_literal(cursor, source, "char");
    default:
        if (clang_isStatement(kind)) {
            return template_of_stmt(cursor, params, source);
        }
        return template_other(cursor);
    }
}

static char *template_of_stmt(CXCursor cursor, const StringArray *params, const char *source) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_CompoundStmt:
        return template_block(cursor, params, source);
    case CXCursor_ReturnStmt:
        return template_return(cursor, params, source);
    case CXCursor_NullStmt:
        return template_simple("empty");
    case CXCursor_DeclStmt:
    case CXCursor_VarDecl:
        return template_simple("let");
    case CXCursor_BinaryOperator:
    case CXCursor_CompoundAssignOperator:
    case CXCursor_CallExpr:
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
    case CXCursor_DeclRefExpr:
    case CXCursor_UnaryOperator:
        return template_of_expr(cursor, params, source);
    default:
        if (clang_isExpression(kind)) {
            return template_of_expr(cursor, params, source);
        }
        return template_other(cursor);
    }
}

static char *body_text_for_function(CXCursor function_cursor, const char *source) {
    CXCursor body = clang_getNullCursor();
    unsigned start = 0;
    unsigned end = 0;
    size_t source_len = strlen(source == NULL ? "" : source);
    const char *open;
    const char *close;

    if (source == NULL) return copy_string("");
    (void)clang_visitChildren(function_cursor, body_finder, &body);
    if (clang_Cursor_isNull(body) || cursor_extent_offsets(body, &start, &end) != 0) {
        return copy_string("");
    }
    if ((size_t)start > source_len) start = (unsigned)source_len;
    if ((size_t)end > source_len) end = (unsigned)source_len;
    if (end < start) end = start;
    open = memchr(source + start, '{', (size_t)(end - start));
    if (open == NULL) return copy_string("");
    close = source + end;
    while (close > open && close[-1] != '}') {
        close--;
    }
    if (close <= open || close[-1] != '}') {
        return copy_string("");
    }
    return trim_copy(open + 1, (size_t)((close - 1) - (open + 1)));
}

static char *ast_template_for_function(CXCursor function_cursor, const StringArray *params, const char *source) {
    CXCursor body = clang_getNullCursor();

    (void)clang_visitChildren(function_cursor, body_finder, &body);
    if (clang_Cursor_isNull(body)) {
        return copy_string("{\"kind\":\"block\",\"stmts\":[]}");
    }
    return template_block(body, params, source);
}

static char *shape_simple(const char *kind) {
    Buf b;
    char *out;

    buf_init(&b);
    if (b.data == NULL) return NULL;
    if (buf_append(&b, "{\"kind\":") != 0 ||
        buf_append_json_string(&b, kind) != 0 ||
        buf_append_char(&b, '}') != 0) {
        buf_free(&b);
        return NULL;
    }
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static char *shape_opaque(void) {
    return shape_simple("opaque");
}

static char *shape_kind_op(const char *kind, const char *op) {
    Buf b;
    char *out;

    buf_init(&b);
    if (b.data == NULL) return NULL;
    if (buf_append(&b, "{\"kind\":") != 0 ||
        buf_append_json_string(&b, kind) != 0 ||
        buf_append(&b, ",\"op\":") != 0 ||
        buf_append_json_string(&b, op) != 0 ||
        buf_append_char(&b, '}') != 0) {
        buf_free(&b);
        return NULL;
    }
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static char *shape_of_stmt(CXCursor cursor);
static char *shape_of_expr(CXCursor cursor);

static char *shape_compound(CXCursor cursor, const char *kind_name) {
    CursorList children = {0};
    Buf b;
    char *out = NULL;

    if (cursor_children(cursor, &children) != 0) {
        return shape_opaque();
    }
    buf_init(&b);
    if (b.data == NULL) goto done;
    if (buf_append(&b, "{\"kind\":") != 0 ||
        buf_append_json_string(&b, kind_name) != 0 ||
        buf_append(&b, ",\"stmts\":[") != 0) {
        goto done;
    }
    for (size_t i = 0; i < children.len; i++) {
        char *child = shape_of_stmt(children.items[i]);

        if (child == NULL) goto done;
        if (i > 0 && buf_append_char(&b, ',') != 0) {
            free(child);
            goto done;
        }
        if (buf_append(&b, child) != 0) {
            free(child);
            goto done;
        }
        free(child);
    }
    if (buf_append(&b, "]}") != 0) goto done;
    out = buf_take(&b);

done:
    cursor_list_free(&children);
    buf_free(&b);
    return out == NULL ? shape_opaque() : out;
}

static char *shape_if(CXCursor cursor) {
    CursorList children = {0};
    char *cond = NULL;
    char *then_shape = NULL;
    char *else_shape = NULL;
    char *out = NULL;
    Buf b = {0};

    if (cursor_children(cursor, &children) != 0) return shape_opaque();
    cond = children.len > 0 ? shape_of_expr(children.items[0]) : shape_opaque();
    then_shape = children.len > 1 ? shape_of_stmt(children.items[1]) : shape_opaque();
    else_shape = children.len > 2 ? shape_of_stmt(children.items[2]) : NULL;
    if (cond == NULL || then_shape == NULL) goto done;

    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"cond\":") != 0 ||
        buf_append(&b, cond) != 0) {
        goto done;
    }
    if (else_shape != NULL &&
        (buf_append(&b, ",\"else\":") != 0 || buf_append(&b, else_shape) != 0)) {
        goto done;
    }
    if (buf_append(&b, ",\"kind\":\"if\",\"then\":") != 0 ||
        buf_append(&b, then_shape) != 0 ||
        buf_append_char(&b, '}') != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(cond);
    free(then_shape);
    free(else_shape);
    cursor_list_free(&children);
    return out == NULL ? shape_opaque() : out;
}

static char *shape_while(CXCursor cursor, int is_do_stmt) {
    CursorList children = {0};
    char *cond = NULL;
    char *body = NULL;
    char *out = NULL;
    Buf b = {0};

    if (cursor_children(cursor, &children) != 0) return shape_opaque();
    if (is_do_stmt) {
        body = children.len > 0 ? shape_of_stmt(children.items[0]) : shape_opaque();
        cond = children.len > 1 ? shape_of_expr(children.items[1]) : shape_opaque();
    } else {
        cond = children.len > 0 ? shape_of_expr(children.items[0]) : shape_opaque();
        body = children.len > 1 ? shape_of_stmt(children.items[1]) : shape_opaque();
    }
    if (cond == NULL || body == NULL) goto done_no_buf;

    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"body\":") != 0 ||
        buf_append(&b, body) != 0 ||
        buf_append(&b, ",\"cond\":") != 0 ||
        buf_append(&b, cond) != 0 ||
        buf_append(&b, ",\"kind\":\"while\"}") != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(cond);
    free(body);
    cursor_list_free(&children);
    return out == NULL ? shape_opaque() : out;
}

static char *shape_for(CXCursor cursor) {
    CursorList children = {0};
    char *body = NULL;
    char *out = NULL;
    Buf b = {0};

    if (cursor_children(cursor, &children) != 0) return shape_opaque();
    body = children.len > 0 ? shape_of_stmt(children.items[children.len - 1]) : shape_opaque();
    if (body == NULL) goto done_no_buf;

    buf_init(&b);
    if (b.data == NULL) goto done_no_buf;
    if (buf_append(&b, "{\"body\":") != 0 ||
        buf_append(&b, body) != 0 ||
        buf_append(&b, ",\"kind\":\"for\"}") != 0) {
        goto done;
    }
    out = buf_take(&b);

done:
    buf_free(&b);
done_no_buf:
    free(body);
    cursor_list_free(&children);
    return out == NULL ? shape_opaque() : out;
}

static char *operator_token(CXCursor cursor, int mode_binary) {
    CXTranslationUnit tu = clang_Cursor_getTranslationUnit(cursor);
    CXSourceRange range = clang_getCursorExtent(cursor);
    CXToken *tokens = NULL;
    unsigned ntoks = 0;
    char *out = NULL;
    unsigned skip_through_offset = 0;
    int have_skip = 0;

    if (mode_binary) {
        CXCursor first;

        if (get_nth_child(cursor, 0, &first)) {
            CXSourceRange first_range = clang_getCursorExtent(first);
            CXSourceLocation end_loc = clang_getRangeEnd(first_range);

            clang_getFileLocation(end_loc, NULL, NULL, NULL, &skip_through_offset);
            have_skip = 1;
        }
    }

    clang_tokenize(tu, range, &tokens, &ntoks);
    for (unsigned i = 0; i < ntoks; i++) {
        if (clang_getTokenKind(tokens[i]) != CXToken_Punctuation) {
            continue;
        }
        if (have_skip) {
            CXSourceLocation tok_loc = clang_getTokenLocation(tu, tokens[i]);
            unsigned tok_offset = 0;

            clang_getFileLocation(tok_loc, NULL, NULL, NULL, &tok_offset);
            if (tok_offset < skip_through_offset) {
                continue;
            }
        }
        out = cx_string_copy(clang_getTokenSpelling(tu, tokens[i]));
        break;
    }
    if (tokens != NULL) {
        clang_disposeTokens(tu, tokens, ntoks);
    }
    return out;
}

static int token_is_assign(const char *tok) {
    size_t n;

    if (tok == NULL) return 0;
    if (strcmp(tok, "=") == 0) return 1;
    if (strcmp(tok, "==") == 0 || strcmp(tok, "!=") == 0 ||
        strcmp(tok, "<=") == 0 || strcmp(tok, ">=") == 0) {
        return 0;
    }
    n = strlen(tok);
    return n >= 2 && tok[n - 1] == '=';
}

static int token_is_rel(const char *tok) {
    return tok != NULL &&
        (strcmp(tok, "==") == 0 || strcmp(tok, "!=") == 0 ||
         strcmp(tok, "<") == 0 || strcmp(tok, "<=") == 0 ||
         strcmp(tok, ">") == 0 || strcmp(tok, ">=") == 0);
}

static int token_is_bin(const char *tok) {
    return tok != NULL &&
        (strcmp(tok, "+") == 0 || strcmp(tok, "-") == 0 ||
         strcmp(tok, "*") == 0 || strcmp(tok, "/") == 0 ||
         strcmp(tok, "%") == 0);
}

static char *shape_binary(CXCursor cursor) {
    char *tok = operator_token(cursor, 1);
    char *out;

    if (token_is_assign(tok)) {
        out = shape_simple("assign");
    } else if (token_is_rel(tok)) {
        out = shape_kind_op("rel", tok);
    } else if (token_is_bin(tok)) {
        out = shape_kind_op("bin", tok);
    } else {
        out = shape_opaque();
    }
    free(tok);
    return out;
}

static char *shape_unwrap_expr(CXCursor cursor) {
    CXCursor child;

    if (get_nth_child(cursor, 0, &child)) {
        return shape_of_expr(child);
    }
    return shape_opaque();
}

static char *shape_of_expr(CXCursor cursor) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_UnexposedExpr:
    case CXCursor_ParenExpr:
        return shape_unwrap_expr(cursor);
    case CXCursor_BinaryOperator:
        return shape_binary(cursor);
    case CXCursor_CompoundAssignOperator:
        return shape_simple("assign");
    case CXCursor_CallExpr:
        return shape_simple("call");
    default:
        return shape_opaque();
    }
}

static char *shape_of_stmt(CXCursor cursor) {
    enum CXCursorKind kind = clang_getCursorKind(cursor);

    switch (kind) {
    case CXCursor_CompoundStmt:
        return shape_compound(cursor, "block");
    case CXCursor_IfStmt:
        return shape_if(cursor);
    case CXCursor_WhileStmt:
        return shape_while(cursor, 0);
    case CXCursor_DoStmt:
        return shape_while(cursor, 1);
    case CXCursor_ForStmt:
        return shape_for(cursor);
    case CXCursor_ReturnStmt:
    case CXCursor_BreakStmt:
    case CXCursor_ContinueStmt:
    case CXCursor_GotoStmt:
        return shape_simple("exit");
    case CXCursor_DeclStmt:
    case CXCursor_VarDecl:
        return shape_simple("let");
    case CXCursor_BinaryOperator:
    case CXCursor_CompoundAssignOperator:
    case CXCursor_CallExpr:
        return shape_of_expr(cursor);
    default:
        if (clang_isExpression(kind)) {
            return shape_of_expr(cursor);
        }
        return shape_opaque();
    }
}

static enum CXChildVisitResult body_finder(CXCursor cursor, CXCursor parent, CXClientData data) {
    CXCursor *body = (CXCursor *)data;

    (void)parent;
    if (clang_getCursorKind(cursor) == CXCursor_CompoundStmt) {
        *body = cursor;
        return CXChildVisit_Break;
    }
    return CXChildVisit_Continue;
}

static char *shape_of_function(CXCursor function_cursor) {
    CXCursor body = clang_getNullCursor();

    (void)clang_visitChildren(function_cursor, body_finder, &body);
    if (clang_Cursor_isNull(body)) {
        return shape_compound(function_cursor, "body");
    }
    return shape_compound(body, "body");
}

static char *trim_copy(const char *s, size_t len) {
    const char *start = s;
    const char *end = s + len;

    while (start < end && isspace((unsigned char)*start)) start++;
    while (end > start && isspace((unsigned char)end[-1])) end--;
    return copy_n(start, (size_t)(end - start));
}

static char *source_line_copy(const char *source, int line_no) {
    const char *start = source;
    int line = 1;

    if (line_no < 1) return NULL;
    while (*start != '\0' && line < line_no) {
        if (*start == '\n') {
            line++;
        }
        start++;
    }
    if (line != line_no) return NULL;
    const char *end = start;
    while (*end != '\0' && *end != '\n') {
        end++;
    }
    if (end > start && end[-1] == '\r') {
        end--;
    }
    return copy_n(start, (size_t)(end - start));
}

static int starts_with(const char *s, const char *prefix) {
    return strncmp(s, prefix, strlen(prefix)) == 0;
}

static char *line_comment_value(const char *line, const char *prefix) {
    const char *start = line + strlen(prefix);
    const char *end;

    while (*start != '\0' && isspace((unsigned char)*start)) start++;
    end = strstr(start, "*/");
    if (end == NULL) {
        end = start + strlen(start);
    }
    return trim_copy(start, (size_t)(end - start));
}

static char *extract_contract_call(const char *line, const char *name) {
    const char *p = strstr(line, name);
    const char *start;
    int depth = 1;

    if (p == NULL) return NULL;
    p += strlen(name);
    while (*p != '\0' && isspace((unsigned char)*p)) p++;
    if (*p != '(') return NULL;
    start = ++p;
    while (*p != '\0') {
        if (*p == '(') {
            depth++;
        } else if (*p == ')') {
            depth--;
            if (depth == 0) {
                return trim_copy(start, (size_t)(p - start));
            }
        }
        p++;
    }
    return NULL;
}

static int is_skippable_annotation_line(const char *line) {
    return line[0] == '\0' ||
        starts_with(line, "//") ||
        starts_with(line, "/*") ||
        starts_with(line, "*") ||
        starts_with(line, "*/") ||
        starts_with(line, "__attribute__") ||
        starts_with(line, "[[") ||
        starts_with(line, "#");
}

static void annotations_free(FunctionAnnotations *ann) {
    if (ann == NULL) return;
    free(ann->concept);
    free(ann->pre);
    free(ann->post);
    memset(ann, 0, sizeof(*ann));
}

static FunctionAnnotations extract_annotations(const char *source, int fn_line) {
    FunctionAnnotations ann = {0};
    int saw_concept = 0;

    for (int line_no = fn_line - 1; line_no >= 1; line_no--) {
        char *raw = source_line_copy(source, line_no);
        char *line;
        char *value;

        if (raw == NULL) break;
        line = trim_copy(raw, strlen(raw));
        free(raw);
        if (line == NULL) break;

        if (starts_with(line, "// concept:")) {
            value = line_comment_value(line, "// concept:");
            if (!saw_concept && value != NULL && !starts_with(value, "UNNAMED-CONCEPT-")) {
                ann.concept = value;
            } else {
                free(value);
            }
            saw_concept = 1;
            free(line);
            continue;
        }
        if (starts_with(line, "/* concept:")) {
            value = line_comment_value(line, "/* concept:");
            if (!saw_concept && value != NULL && !starts_with(value, "UNNAMED-CONCEPT-")) {
                ann.concept = value;
            } else {
                free(value);
            }
            saw_concept = 1;
            free(line);
            continue;
        }
        value = extract_contract_call(line, "@requires");
        if (value != NULL) {
            if (ann.pre == NULL) ann.pre = value;
            else free(value);
            free(line);
            continue;
        }
        value = extract_contract_call(line, "@ensures");
        if (value != NULL) {
            if (ann.post == NULL) ann.post = value;
            else free(value);
            free(line);
            continue;
        }
        if (is_skippable_annotation_line(line)) {
            free(line);
            continue;
        }
        free(line);
        break;
    }
    return ann;
}

static char *normalized_type_spelling(CXType type) {
    char *raw = cx_string_copy(clang_getTypeSpelling(type));
    Buf b;
    int prev_ws = 0;
    char *out;

    if (raw == NULL) return NULL;
    buf_init(&b);
    if (b.data == NULL) {
        free(raw);
        return NULL;
    }
    for (const unsigned char *p = (const unsigned char *)raw; *p != '\0'; p++) {
        if (isspace(*p)) {
            prev_ws = 1;
        } else {
            if (prev_ws && b.len > 0 && b.data[b.len - 1] != ' ') {
                if (buf_append_char(&b, ' ') != 0) {
                    free(raw);
                    buf_free(&b);
                    return NULL;
                }
            }
            if (buf_append_char(&b, (char)*p) != 0) {
                free(raw);
                buf_free(&b);
                return NULL;
            }
            prev_ws = 0;
        }
    }
    free(raw);
    out = buf_take(&b);
    buf_free(&b);
    return out;
}

static int append_param_arrays(Buf *names, Buf *types, CXCursor function_cursor) {
    int n = clang_Cursor_getNumArguments(function_cursor);

    if (n < 0) n = 0;
    if (buf_append_char(names, '[') != 0 || buf_append_char(types, '[') != 0) {
        return -1;
    }
    for (int i = 0; i < n; i++) {
        CXCursor arg = clang_Cursor_getArgument(function_cursor, (unsigned)i);
        CXType arg_type = clang_getCursorType(arg);
        char fallback[32];
        char *name = cx_string_copy(clang_getCursorSpelling(arg));
        char *type = normalized_type_spelling(arg_type);

        if (name == NULL || type == NULL) {
            free(name);
            free(type);
            return -1;
        }
        if (name[0] == '\0') {
            free(name);
            (void)snprintf(fallback, sizeof(fallback), "__arg%d", i);
            name = copy_string(fallback);
            if (name == NULL) {
                free(type);
                return -1;
            }
        }
        if (i > 0 &&
            (buf_append_char(names, ',') != 0 || buf_append_char(types, ',') != 0)) {
            free(name);
            free(type);
            return -1;
        }
        if (buf_append_json_string(names, name) != 0 ||
            buf_append_json_string(types, type) != 0) {
            free(name);
            free(type);
            return -1;
        }
        free(name);
        free(type);
    }
    return buf_append_char(names, ']') == 0 && buf_append_char(types, ']') == 0 ? 0 : -1;
}

static int emit_function_entry(FileLiftCtx *ctx, CXCursor cursor) {
    char *fn_name = cx_string_copy(clang_getCursorSpelling(cursor));
    CXSourceLocation loc = clang_getCursorLocation(cursor);
    unsigned line = 1;
    StringArray param_names = {0};
    Buf names;
    Buf types;
    Buf entry;
    char *return_type = NULL;
    char *term_shape = NULL;
    char *term_shape_cid = NULL;
    char *body_text = NULL;
    char *body_source_cid = NULL;
    char *ast_template = NULL;
    char *template_cid = NULL;
    FunctionAnnotations ann;
    int ok;

    memset(&ann, 0, sizeof(ann));
    clang_getSpellingLocation(loc, NULL, &line, NULL, NULL);
    if (fn_name == NULL) return -1;

    buf_init(&names);
    buf_init(&types);
    buf_init(&entry);
    if (names.data == NULL || types.data == NULL || entry.data == NULL) {
        free(fn_name);
        buf_free(&names);
        buf_free(&types);
        buf_free(&entry);
        return -1;
    }
    if (collect_param_names(cursor, &param_names) != 0) {
        free(fn_name);
        string_array_free(&param_names);
        buf_free(&names);
        buf_free(&types);
        buf_free(&entry);
        return -1;
    }
    if (append_param_arrays(&names, &types, cursor) != 0) {
        free(fn_name);
        string_array_free(&param_names);
        buf_free(&names);
        buf_free(&types);
        buf_free(&entry);
        return -1;
    }

    CXType result_type = clang_getResultType(clang_getCursorType(cursor));
    return_type = result_type.kind == CXType_Void ? copy_string("()") : normalized_type_spelling(result_type);
    term_shape = shape_of_function(cursor);
    term_shape_cid = term_shape == NULL ? NULL : cid_for_bytes(term_shape, strlen(term_shape));
    body_text = body_text_for_function(cursor, ctx->source);
    body_source_cid = body_text == NULL ? NULL : cid_for_bytes(body_text, strlen(body_text));
    ast_template = ast_template_for_function(cursor, &param_names, ctx->source);
    template_cid = ast_template == NULL ? NULL : cid_for_bytes(ast_template, strlen(ast_template));
    ann = extract_annotations(ctx->source, (int)line);
    if (return_type == NULL || term_shape == NULL || term_shape_cid == NULL ||
        body_text == NULL || body_source_cid == NULL ||
        ast_template == NULL || template_cid == NULL) {
        free(fn_name);
        free(return_type);
        free(term_shape);
        free(term_shape_cid);
        free(body_text);
        free(body_source_cid);
        free(ast_template);
        free(template_cid);
        annotations_free(&ann);
        string_array_free(&param_names);
        buf_free(&names);
        buf_free(&types);
        buf_free(&entry);
        return -1;
    }

    ok = buf_append(&entry, "{\"attr_post\":") == 0;
    if (ok && ann.post != NULL) ok = buf_append_json_string(&entry, ann.post) == 0;
    else if (ok) ok = buf_append(&entry, "null") == 0;
    if (ok) ok = buf_append(&entry, ",\"attr_pre\":") == 0;
    if (ok && ann.pre != NULL) ok = buf_append_json_string(&entry, ann.pre) == 0;
    else if (ok) ok = buf_append(&entry, "null") == 0;
    if (ok) ok = buf_append(&entry, ",\"concept_annotation\":") == 0;
    if (ok && ann.concept != NULL) ok = buf_append_json_string(&entry, ann.concept) == 0;
    else if (ok) ok = buf_append(&entry, "null") == 0;
    if (ok) ok = buf_append(&entry, ",\"file\":") == 0 &&
        buf_append_json_string(&entry, ctx->rel) == 0 &&
        buf_append(&entry, ",\"fn_line\":") == 0 &&
        buf_append_uint(&entry, line) == 0 &&
        buf_append(&entry, ",\"fn_name\":") == 0 &&
        buf_append_json_string(&entry, fn_name) == 0 &&
        buf_append(&entry, ",\"kind\":\"bind-lift-entry\",\"param_names\":") == 0 &&
        buf_append(&entry, names.data) == 0 &&
        buf_append(&entry, ",\"param_types\":") == 0 &&
        buf_append(&entry, types.data) == 0 &&
        buf_append(&entry, ",\"return_type\":") == 0 &&
        buf_append_json_string(&entry, return_type) == 0 &&
        buf_append(&entry, ",\"term_shape\":") == 0 &&
        buf_append(&entry, term_shape) == 0 &&
        buf_append(&entry, ",\"term_shape_cid\":") == 0 &&
        buf_append_json_string(&entry, term_shape_cid) == 0 &&
        buf_append(&entry, ",\"body_source\":{\"ast_template\":") == 0 &&
        buf_append(&entry, ast_template) == 0 &&
        buf_append(&entry, ",\"body_text\":") == 0 &&
        buf_append_json_string(&entry, body_text) == 0 &&
        buf_append(&entry, ",\"file\":") == 0 &&
        buf_append_json_string(&entry, ctx->rel) == 0 &&
        buf_append(&entry, ",\"param_names\":") == 0 &&
        buf_append(&entry, names.data) == 0 &&
        buf_append(&entry, ",\"source_cid\":") == 0 &&
        buf_append_json_string(&entry, body_source_cid) == 0 &&
        buf_append(&entry, ",\"span\":") == 0 &&
        append_span_object(&entry, cursor) == 0 &&
        buf_append(&entry, ",\"template_cid\":") == 0 &&
        buf_append_json_string(&entry, template_cid) == 0 &&
        buf_append_char(&entry, '}') == 0 &&
        buf_append_char(&entry, '}') == 0;

    if (!ok || acc_append_json_item(&ctx->acc->ir, &ctx->acc->ir_count, entry.data) != 0) {
        free(fn_name);
        free(return_type);
        free(term_shape);
        free(term_shape_cid);
        free(body_text);
        free(body_source_cid);
        free(ast_template);
        free(template_cid);
        annotations_free(&ann);
        string_array_free(&param_names);
        buf_free(&names);
        buf_free(&types);
        buf_free(&entry);
        return -1;
    }

    free(fn_name);
    free(return_type);
    free(term_shape);
    free(term_shape_cid);
    free(body_text);
    free(body_source_cid);
    free(ast_template);
    free(template_cid);
    annotations_free(&ann);
    string_array_free(&param_names);
    buf_free(&names);
    buf_free(&types);
    buf_free(&entry);
    return 0;
}

static enum CXChildVisitResult visit_function(CXCursor cursor, CXCursor parent, CXClientData data) {
    FileLiftCtx *ctx = (FileLiftCtx *)data;

    (void)parent;
    if (ctx->failed || !clang_Location_isFromMainFile(clang_getCursorLocation(cursor))) {
        return CXChildVisit_Continue;
    }
    if (clang_getCursorKind(cursor) == CXCursor_FunctionDecl &&
        clang_isCursorDefinition(cursor) != 0) {
        if (emit_function_entry(ctx, cursor) != 0) {
            ctx->failed = 1;
            return CXChildVisit_Break;
        }
        return CXChildVisit_Continue;
    }
    return CXChildVisit_Recurse;
}

static CXTranslationUnit parse_unit(
    const char *path,
    const char *source,
    const StringArray *clang_args,
    CXIndex *index_out
) {
    static const char *const default_args[] = {"-x", "c", "-std=c11"};
    const char *const *args = default_args;
    int n_args = (int)(sizeof(default_args) / sizeof(default_args[0]));
    struct CXUnsavedFile unsaved;
    CXTranslationUnit unit = NULL;

    if (clang_args != NULL && clang_args->len > 0) {
        args = (const char *const *)clang_args->items;
        n_args = (int)clang_args->len;
    }
    unsaved.Filename = path;
    unsaved.Contents = source == NULL ? "" : source;
    unsaved.Length = (unsigned long)strlen(source == NULL ? "" : source);

    *index_out = clang_createIndex(0, 0);
    if (*index_out == NULL) return NULL;
    if (clang_parseTranslationUnit2(
            *index_out,
            path,
            args,
            n_args,
            &unsaved,
            1,
            CXTranslationUnit_DetailedPreprocessingRecord |
                CXTranslationUnit_Incomplete |
                CXTranslationUnit_KeepGoing,
            &unit) != CXError_Success) {
        unit = NULL;
    }
    return unit;
}

typedef struct {
    const char *rel;
    const char *source;
    const BindingList *bindings;
    Buf *tags;
    size_t *tag_count;
    int failed;
} RecognizeCtx;

static int append_param_bindings(Buf *b, const StringArray *params) {
    if (buf_append_char(b, '[') != 0) return -1;
    if (params != NULL) {
        for (size_t i = 0; i < params->len; i++) {
            if (i > 0 && buf_append_char(b, ',') != 0) return -1;
            if (buf_append(b, "{\"index\":") != 0 ||
                buf_append_uint(b, (unsigned)i + 1u) != 0 ||
                buf_append(b, ",\"source_text\":") != 0 ||
                buf_append_json_string(b, params->items[i]) != 0 ||
                buf_append_char(b, '}') != 0) {
                return -1;
            }
        }
    }
    return buf_append_char(b, ']');
}

static int append_recognize_tag(
    RecognizeCtx *ctx,
    CXCursor cursor,
    const char *fn_name,
    const StringArray *params,
    const char *template_cid,
    const BindingTemplate *binding
) {
    Buf tag;
    int ok;

    buf_init(&tag);
    if (tag.data == NULL) return -1;
    ok = buf_append(&tag, "{\"concept_name\":") == 0 &&
        buf_append(&tag, binding->concept_name_json) == 0 &&
        buf_append(&tag, ",\"contract_cid\":") == 0 &&
        buf_append(&tag, binding->contract_cid_json) == 0 &&
        buf_append(&tag, ",\"family\":") == 0 &&
        buf_append(&tag, binding->family_json) == 0 &&
        buf_append(&tag, ",\"file\":") == 0 &&
        buf_append_json_string(&tag, ctx->rel) == 0 &&
        buf_append(&tag, ",\"function_name\":") == 0 &&
        buf_append_json_string(&tag, fn_name) == 0 &&
        buf_append(&tag, ",\"library_tag\":") == 0 &&
        buf_append(&tag, binding->library_tag_json) == 0 &&
        buf_append(&tag, ",\"match_tier\":\"exact\",\"param_bindings\":") == 0 &&
        append_param_bindings(&tag, params) == 0 &&
        buf_append(&tag, ",\"span\":") == 0 &&
        append_span_object(&tag, cursor) == 0 &&
        buf_append(&tag, ",\"template_cid\":") == 0 &&
        buf_append_json_string(&tag, template_cid) == 0 &&
        buf_append_char(&tag, '}') == 0;
    if (!ok || acc_append_json_item(ctx->tags, ctx->tag_count, tag.data) != 0) {
        buf_free(&tag);
        return -1;
    }
    buf_free(&tag);
    return 0;
}

static int recognize_function(RecognizeCtx *ctx, CXCursor cursor) {
    char *fn_name = cx_string_copy(clang_getCursorSpelling(cursor));
    StringArray params = {0};
    char *template_json = NULL;
    char *template_cid = NULL;
    const BindingTemplate *binding;
    int rc = 0;

    if (fn_name == NULL) return -1;
    if (collect_param_names(cursor, &params) != 0) {
        free(fn_name);
        return -1;
    }
    template_json = ast_template_for_function(cursor, &params, ctx->source);
    template_cid = template_json == NULL ? NULL : cid_for_bytes(template_json, strlen(template_json));
    if (template_json == NULL || template_cid == NULL) {
        rc = -1;
        goto done;
    }
    binding = binding_for_template_cid(ctx->bindings, template_cid);
    if (binding != NULL) {
        rc = append_recognize_tag(ctx, cursor, fn_name, &params, template_cid, binding);
    }

done:
    free(fn_name);
    string_array_free(&params);
    free(template_json);
    free(template_cid);
    return rc;
}

static enum CXChildVisitResult recognize_visit_function(CXCursor cursor, CXCursor parent, CXClientData data) {
    RecognizeCtx *ctx = (RecognizeCtx *)data;

    (void)parent;
    if (ctx->failed || !clang_Location_isFromMainFile(clang_getCursorLocation(cursor))) {
        return CXChildVisit_Continue;
    }
    if (clang_getCursorKind(cursor) == CXCursor_FunctionDecl &&
        clang_isCursorDefinition(cursor) != 0) {
        if (recognize_function(ctx, cursor) != 0) {
            ctx->failed = 1;
            return CXChildVisit_Break;
        }
        return CXChildVisit_Continue;
    }
    return CXChildVisit_Recurse;
}

static int recognize_one_file(
    const char *project_root,
    const char *rel,
    const char *path,
    const BindingList *bindings,
    Buf *tags,
    size_t *tag_count
) {
    char *source = read_file(path);
    CXIndex index = NULL;
    CXTranslationUnit unit;
    RecognizeCtx ctx;

    (void)project_root;
    if (source == NULL) {
        return 0;
    }
    unit = parse_unit(path, source, NULL, &index);
    if (unit == NULL) {
        free(source);
        if (index != NULL) clang_disposeIndex(index);
        return 0;
    }

    memset(&ctx, 0, sizeof(ctx));
    ctx.rel = rel;
    ctx.source = source;
    ctx.bindings = bindings;
    ctx.tags = tags;
    ctx.tag_count = tag_count;
    (void)clang_visitChildren(clang_getTranslationUnitCursor(unit), recognize_visit_function, &ctx);

    clang_disposeTranslationUnit(unit);
    clang_disposeIndex(index);
    free(source);
    return ctx.failed ? -1 : 0;
}

static int lift_one_file(
    const char *workspace,
    const char *path,
    LiftAccumulator *acc,
    const StringArray *clang_args
) {
    char *source = read_file(path);
    char *rel = relative_path(workspace, path);
    CXIndex index = NULL;
    CXTranslationUnit unit;
    FileLiftCtx ctx;

    if (rel == NULL) return -1;
    if (source == NULL) {
        int rc = acc_add_diagnostic(acc, "read-error", rel, strerror(errno));
        free(rel);
        return rc;
    }
    if (scan_concept_citations(acc, workspace, rel, source) != 0) {
        free(source);
        free(rel);
        return -1;
    }
    unit = parse_unit(path, source, clang_args, &index);
    if (unit == NULL) {
        (void)acc_add_diagnostic(acc, "parse-error", rel, "libclang parse failed");
        free(source);
        free(rel);
        if (index != NULL) clang_disposeIndex(index);
        return 0;
    }

    memset(&ctx, 0, sizeof(ctx));
    ctx.path = path;
    ctx.rel = rel;
    ctx.source = source;
    ctx.acc = acc;
    (void)ctx.path;
    (void)clang_visitChildren(clang_getTranslationUnitCursor(unit), visit_function, &ctx);

    clang_disposeTranslationUnit(unit);
    clang_disposeIndex(index);
    free(source);
    free(rel);
    if (ctx.failed) {
        (void)snprintf(acc->error, sizeof(acc->error), "%s: lift failed", path);
        return -1;
    }
    return 0;
}

#else

static int lift_one_file(
    const char *workspace,
    const char *path,
    LiftAccumulator *acc,
    const StringArray *clang_args
) {
    char *rel = relative_path(workspace, path);
    char *source;
    int rc;

    (void)clang_args;
    if (rel == NULL) return -1;
    source = read_file(path);
    if (source == NULL) {
        rc = acc_add_diagnostic(acc, "read-error", rel, strerror(errno));
        free(rel);
        return rc;
    }
    rc = scan_concept_citations(acc, workspace, rel, source);
    free(source);
    if (rc != 0) {
        free(rel);
        return rc;
    }
    rc = acc_add_diagnostic(acc, "unavailable", rel, "libclang support is not enabled");
    free(rel);
    return rc;
}

static int recognize_one_file(
    const char *project_root,
    const char *rel,
    const char *path,
    const BindingList *bindings,
    Buf *tags,
    size_t *tag_count
) {
    (void)project_root;
    (void)rel;
    (void)path;
    (void)bindings;
    (void)tags;
    (void)tag_count;
    return -2;
}

#endif

static int walk_path(
    const char *workspace,
    const char *path,
    LiftAccumulator *acc,
    const StringArray *clang_args
) {
    struct stat st;
    DIR *dir;
    struct dirent *entry;

    if (stat(path, &st) != 0) {
        char *rel = relative_path(workspace, path);
        int rc = acc_add_diagnostic(acc, "stat-error", rel == NULL ? path : rel, strerror(errno));

        free(rel);
        return rc;
    }
    if (S_ISREG(st.st_mode)) {
        return has_suffix(path, ".c") ? lift_one_file(workspace, path, acc, clang_args) : 0;
    }
    if (!S_ISDIR(st.st_mode)) {
        return 0;
    }
    dir = opendir(path);
    if (dir == NULL) {
        char *rel = relative_path(workspace, path);
        int rc = acc_add_diagnostic(acc, "opendir-error", rel == NULL ? path : rel, strerror(errno));

        free(rel);
        return rc;
    }
    while ((entry = readdir(dir)) != NULL) {
        char *child;
        int rc;

        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        child = join_path(path, entry->d_name);
        if (child == NULL) {
            closedir(dir);
            return -1;
        }
        rc = walk_path(workspace, child, acc, clang_args);
        free(child);
        if (rc != 0) {
            closedir(dir);
            return rc;
        }
    }
    closedir(dir);
    return 0;
}

static void send_response(const char *id, const char *result_json) {
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"result\":%s}\n", id == NULL ? "null" : id, result_json);
    fflush(stdout);
}

static void send_error(const char *id, int code, const char *message) {
    Buf b;
    char code_buf[32];

    buf_init(&b);
    if (b.data == NULL) {
        printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":{\"code\":-32603,\"message\":\"internal error\"}}\n",
            id == NULL ? "null" : id);
        fflush(stdout);
        return;
    }
    (void)snprintf(code_buf, sizeof(code_buf), "%d", code);
    (void)buf_append(&b, "{\"code\":");
    (void)buf_append(&b, code_buf);
    (void)buf_append(&b, ",\"message\":");
    (void)buf_append_json_string(&b, message);
    (void)buf_append_char(&b, '}');
    printf("{\"jsonrpc\":\"2.0\",\"id\":%s,\"error\":%s}\n", id == NULL ? "null" : id, b.data);
    fflush(stdout);
    buf_free(&b);
}

static void handle_initialize(const char *id) {
    send_response(id,
        "{\"capabilities\":{\"authoring_surfaces\":[\"c\",\"c11\",\"c-bind\"],"
        "\"emits_signed_mementos\":false,\"ir_version\":\"bind-ir/1.0.0\"},"
        "\"name\":\"provekit-bind-lift-c\",\"protocol_version\":\"pep/1.7.0\","
        "\"version\":\"0.1.0\"}");
}

static void handle_lift(const char *id, const char *line) {
    char *workspace = json_extract_param_str(line, "workspace_root");
    StringArray source_paths = {0};
    StringArray clang_args = {0};
    LiftAccumulator acc;
    Buf result;

    if (workspace == NULL || workspace[0] == '\0') {
        free(workspace);
        workspace = copy_string(".");
        if (workspace == NULL) {
            send_error(id, -32603, "out of memory");
            return;
        }
    }
    if (json_extract_param_str_array(line, "source_paths", &source_paths) != 0) {
        free(workspace);
        string_array_free(&source_paths);
        send_error(id, -32602, "source_paths must be an array of strings");
        return;
    }
    if (source_paths.len == 0 && string_array_push_copy(&source_paths, ".") != 0) {
        free(workspace);
        send_error(id, -32603, "out of memory");
        return;
    }
    if (json_extract_param_str_array(line, "clang_args", &clang_args) != 0) {
        free(workspace);
        string_array_free(&source_paths);
        string_array_free(&clang_args);
        send_error(id, -32602, "clang_args must be an array of strings");
        return;
    }

    acc_init(&acc);
    if (acc.ir.data == NULL || acc.concept_citations.data == NULL || acc.diagnostics.data == NULL) {
        acc_free(&acc);
        free(workspace);
        string_array_free(&source_paths);
        string_array_free(&clang_args);
        send_error(id, -32603, "out of memory");
        return;
    }
    for (size_t i = 0; i < source_paths.len; i++) {
        char *resolved = resolve_source_path(workspace, source_paths.items[i]);
        int rc;

        if (resolved == NULL) {
            acc_free(&acc);
            free(workspace);
            string_array_free(&source_paths);
            string_array_free(&clang_args);
            send_error(id, -32603, "out of memory");
            return;
        }
        rc = walk_path(workspace, resolved, &acc, &clang_args);
        free(resolved);
        if (rc != 0) {
            send_error(id, -32603, acc.error[0] ? acc.error : "lift failed");
            acc_free(&acc);
            free(workspace);
            string_array_free(&source_paths);
            string_array_free(&clang_args);
            return;
        }
    }

    buf_init(&result);
    if (result.data == NULL ||
        buf_append(&result, "{\"diagnostics\":[") != 0 ||
        buf_append(&result, acc.diagnostics.data == NULL ? "" : acc.diagnostics.data) != 0 ||
        buf_append(&result, "],\"concept_citations\":[") != 0 ||
        buf_append(&result, acc.concept_citations.data == NULL ? "" : acc.concept_citations.data) != 0 ||
        buf_append(&result, "],\"ir\":[") != 0 ||
        buf_append(&result, acc.ir.data == NULL ? "" : acc.ir.data) != 0 ||
        buf_append(&result, "],\"kind\":\"ir-document\"}") != 0) {
        buf_free(&result);
        acc_free(&acc);
        free(workspace);
        string_array_free(&source_paths);
        string_array_free(&clang_args);
        send_error(id, -32603, "out of memory");
        return;
    }
    send_response(id, result.data);

    buf_free(&result);
    acc_free(&acc);
    free(workspace);
    string_array_free(&source_paths);
    string_array_free(&clang_args);
}

static void handle_recognize(const char *id, const char *line) {
    char *project_root = json_extract_param_str(line, "project_root");
    StringArray source_paths = {0};
    BindingList bindings = {0};
    Buf tags;
    Buf result;
    size_t tag_count = 0;

    if (project_root == NULL || project_root[0] == '\0') {
        free(project_root);
        send_error(id, -32602, "missing `project_root`");
        return;
    }
    if (json_extract_param_str_array(line, "source_paths", &source_paths) != 0) {
        free(project_root);
        string_array_free(&source_paths);
        send_error(id, -32602, "source_paths must be an array of strings");
        return;
    }
    if (json_extract_param_binding_templates(line, &bindings) != 0) {
        free(project_root);
        string_array_free(&source_paths);
        binding_list_free(&bindings);
        send_error(id, -32602, "binding_templates must be an array of objects");
        return;
    }

    buf_init(&tags);
    if (tags.data == NULL) {
        free(project_root);
        string_array_free(&source_paths);
        binding_list_free(&bindings);
        send_error(id, -32603, "out of memory");
        return;
    }

    for (size_t i = 0; i < source_paths.len; i++) {
        char *resolved = resolve_source_path(project_root, source_paths.items[i]);
        int rc;

        if (resolved == NULL) {
            buf_free(&tags);
            free(project_root);
            string_array_free(&source_paths);
            binding_list_free(&bindings);
            send_error(id, -32603, "out of memory");
            return;
        }
        rc = recognize_one_file(
            project_root,
            source_paths.items[i],
            resolved,
            &bindings,
            &tags,
            &tag_count);
        free(resolved);
        if (rc == -2) {
            buf_free(&tags);
            free(project_root);
            string_array_free(&source_paths);
            binding_list_free(&bindings);
            send_error(id, -32603, "recognize: libclang support is not enabled");
            return;
        }
        if (rc != 0) {
            buf_free(&tags);
            free(project_root);
            string_array_free(&source_paths);
            binding_list_free(&bindings);
            send_error(id, -32603, "recognize failed");
            return;
        }
    }

    buf_init(&result);
    if (result.data == NULL ||
        buf_append(&result, "{\"tags\":[") != 0 ||
        buf_append(&result, tags.data == NULL ? "" : tags.data) != 0 ||
        buf_append(&result, "]}") != 0) {
        buf_free(&result);
        buf_free(&tags);
        free(project_root);
        string_array_free(&source_paths);
        binding_list_free(&bindings);
        send_error(id, -32603, "out of memory");
        return;
    }
    send_response(id, result.data);

    buf_free(&result);
    buf_free(&tags);
    free(project_root);
    string_array_free(&source_paths);
    binding_list_free(&bindings);
}

int main(int argc, char **argv) {
    char *line = NULL;
    size_t line_cap = 0;

    if (argc > 2 ||
        (argc == 2 && strcmp(argv[1], "--rpc") != 0 && strcmp(argv[1], "--bind-rpc") != 0)) {
        fprintf(stderr, "usage: %s [--rpc|--bind-rpc]\n", argv[0]);
        return 1;
    }

    while (getline(&line, &line_cap, stdin) != -1) {
        char *id;
        char *method;

        if (!validate_json_request(line)) {
            send_error("null", -32700, "parse error");
            continue;
        }
        id = json_extract_id(line);
        method = json_extract_str(line, "method");
        if (id == NULL) {
            id = copy_string("null");
        }
        if (method == NULL) {
            send_error(id, -32600, "missing method");
        } else if (strcmp(method, "initialize") == 0) {
            handle_initialize(id);
        } else if (strcmp(method, "lift") == 0) {
            handle_lift(id, line);
        } else if (strcmp(method, "provekit.plugin.recognize") == 0) {
            handle_recognize(id, line);
        } else if (strcmp(method, "shutdown") == 0) {
            send_response(id, "null");
            free(method);
            free(id);
            break;
        } else {
            send_error(id, -32601, "unknown method");
        }
        free(method);
        free(id);
    }
    free(line);
    return 0;
}
