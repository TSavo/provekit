#include "provekit/c_lift_core.h"

#include <ctype.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char **items;
    size_t len;
    size_t cap;
} pk_c_cc_words;

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} pk_c_cc_buf;

static int pk_c_cc_is_absolute_path(const char *path);
static char *pk_c_cc_join_path(const char *root, const char *path);

static char *pk_c_cc_copy_n(const char *src, size_t len) {
    char *copy = malloc(len + 1);

    if (copy == NULL) {
        return NULL;
    }
    memcpy(copy, src, len);
    copy[len] = '\0';
    return copy;
}

static char *pk_c_cc_copy(const char *src) {
    if (src == NULL) {
        return NULL;
    }
    return pk_c_cc_copy_n(src, strlen(src));
}

static int pk_c_cc_checked_mul(size_t lhs, size_t rhs, size_t *out) {
    if (lhs != 0 && rhs > SIZE_MAX / lhs) {
        return -1;
    }
    *out = lhs * rhs;
    return 0;
}

static char *pk_c_cc_trim_copy(const char *src) {
    const char *start = src == NULL ? "" : src;
    const char *end;

    while (*start != '\0' && isspace((unsigned char)*start)) {
        start++;
    }
    end = start + strlen(start);
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    return pk_c_cc_copy_n(start, (size_t)(end - start));
}

static int pk_c_cc_buf_append(pk_c_cc_buf *buf, char c) {
    char *data;
    size_t cap;

    if (buf->len + 1 >= buf->cap) {
        cap = buf->cap == 0 ? 16 : buf->cap * 2;
        if (cap < buf->cap) {
            return -1;
        }
        data = realloc(buf->data, cap);
        if (data == NULL) {
            return -1;
        }
        buf->data = data;
        buf->cap = cap;
    }
    buf->data[buf->len++] = c;
    return 0;
}

static int pk_c_cc_words_add_owned(pk_c_cc_words *words, char *item) {
    char **items;
    size_t cap;
    size_t bytes;

    if (words->len >= words->cap) {
        cap = words->cap == 0 ? 8 : words->cap * 2;
        if (cap < words->cap ||
            pk_c_cc_checked_mul(cap, sizeof(*words->items), &bytes) != 0) {
            free(item);
            return -1;
        }
        items = realloc(words->items, bytes);
        if (items == NULL) {
            free(item);
            return -1;
        }
        words->items = items;
        words->cap = cap;
    }
    words->items[words->len++] = item;
    return 0;
}

static void pk_c_cc_words_free(pk_c_cc_words *words) {
    if (words == NULL) {
        return;
    }
    for (size_t i = 0; i < words->len; i++) {
        free(words->items[i]);
    }
    free(words->items);
    words->items = NULL;
    words->len = 0;
    words->cap = 0;
}

static int pk_c_cc_split_command(const char *command, pk_c_cc_words *words, int *unclosed_quote) {
    const char *p = command == NULL ? "" : command;

    if (unclosed_quote != NULL) {
        *unclosed_quote = 0;
    }
    while (*p != '\0') {
        pk_c_cc_buf buf = {0};
        int quote = 0;

        while (*p != '\0' && isspace((unsigned char)*p)) {
            p++;
        }
        if (*p == '\0') {
            break;
        }
        while (*p != '\0') {
            unsigned char c = (unsigned char)*p;

            if (quote == 0 && isspace(c)) {
                break;
            }
            if (quote != 0) {
                if (*p == quote) {
                    quote = 0;
                    p++;
                    continue;
                }
                if (quote == '"' && *p == '\\' && p[1] != '\0') {
                    p++;
                    if (pk_c_cc_buf_append(&buf, *p++) != 0) {
                        free(buf.data);
                        return -1;
                    }
                    continue;
                }
                if (pk_c_cc_buf_append(&buf, *p++) != 0) {
                    free(buf.data);
                    return -1;
                }
                continue;
            }
            if (*p == '\'' || *p == '"') {
                quote = *p++;
                continue;
            }
            if (*p == '\\' && p[1] != '\0') {
                p++;
                if (pk_c_cc_buf_append(&buf, *p++) != 0) {
                    free(buf.data);
                    return -1;
                }
                continue;
            }
            if (pk_c_cc_buf_append(&buf, *p++) != 0) {
                free(buf.data);
                return -1;
            }
        }
        if (quote != 0 && unclosed_quote != NULL) {
            *unclosed_quote = 1;
        }
        if (pk_c_cc_buf_append(&buf, '\0') != 0) {
            free(buf.data);
            return -1;
        }
        if (pk_c_cc_words_add_owned(words, buf.data) != 0) {
            return -1;
        }
    }
    return 0;
}

static pk_c_compile_context *pk_c_cc_context_new(const char *command) {
    pk_c_compile_context *context = calloc(1, sizeof(*context));

    if (context == NULL) {
        return NULL;
    }
    context->compile_command = pk_c_cc_trim_copy(command);
    if (context->compile_command == NULL) {
        pk_c_compile_context_free(context);
        return NULL;
    }
    return context;
}

static int pk_c_cc_add_arg(pk_c_compile_context *context, const char *arg) {
    char **items;
    char *copy;
    size_t cap;
    size_t bytes;

    if (context == NULL || arg == NULL) {
        return -1;
    }
    if (context->n_clang_args >= context->cap_clang_args) {
        cap = context->cap_clang_args == 0 ? 8 : context->cap_clang_args * 2;
        if (cap < context->cap_clang_args ||
            pk_c_cc_checked_mul(cap, sizeof(*context->clang_args), &bytes) != 0) {
            return -1;
        }
        items = realloc(context->clang_args, bytes);
        if (items == NULL) {
            return -1;
        }
        context->clang_args = items;
        context->cap_clang_args = cap;
    }
    copy = pk_c_cc_copy(arg);
    if (copy == NULL) {
        return -1;
    }
    context->clang_args[context->n_clang_args++] = copy;
    return 0;
}

static int pk_c_cc_set_target(pk_c_compile_context *context, const char *target) {
    char *copy;

    if (target == NULL || target[0] == '\0') {
        return 0;
    }
    copy = pk_c_cc_copy(target);
    if (copy == NULL) {
        return -1;
    }
    free(context->target_triple);
    context->target_triple = copy;
    return 0;
}

static int pk_c_cc_has_prefix(const char *str, const char *prefix) {
    return strncmp(str, prefix, strlen(prefix)) == 0;
}

static int pk_c_cc_ends_with(const char *str, const char *suffix) {
    size_t str_len = strlen(str);
    size_t suffix_len = strlen(suffix);

    return str_len >= suffix_len &&
        strcmp(str + str_len - suffix_len, suffix) == 0;
}

static const char *pk_c_cc_basename(const char *path) {
    const char *slash = strrchr(path, '/');

    return slash == NULL ? path : slash + 1;
}

static int pk_c_cc_is_wrapper(const char *word) {
    const char *base = pk_c_cc_basename(word);

    return strcmp(base, "ccache") == 0 ||
        strcmp(base, "sccache") == 0 ||
        strcmp(base, "distcc") == 0;
}

static size_t pk_c_cc_first_arg_index(const pk_c_cc_words *words) {
    size_t i = 0;

    while (i < words->len && pk_c_cc_is_wrapper(words->items[i])) {
        i++;
    }
    if (i < words->len) {
        return i + 1;
    }
    return words->len;
}

static int pk_c_cc_is_source_input(const char *arg) {
    if (arg[0] == '-') {
        return 0;
    }
    return pk_c_cc_ends_with(arg, ".c") ||
        pk_c_cc_ends_with(arg, ".h") ||
        pk_c_cc_ends_with(arg, ".i") ||
        pk_c_cc_ends_with(arg, ".s") ||
        pk_c_cc_ends_with(arg, ".S");
}

static int pk_c_cc_drop_with_next(const char *arg) {
    return strcmp(arg, "-o") == 0 ||
        strcmp(arg, "-MF") == 0 ||
        strcmp(arg, "-MT") == 0 ||
        strcmp(arg, "-MQ") == 0 ||
        strcmp(arg, "-MJ") == 0 ||
        strcmp(arg, "-dumpbase") == 0 ||
        strcmp(arg, "-dumpdir") == 0 ||
        strcmp(arg, "-auxbase") == 0 ||
        strcmp(arg, "-auxbase-strip") == 0;
}

static int pk_c_cc_drop_attached(const char *arg) {
    return pk_c_cc_has_prefix(arg, "-Wp,") ||
        pk_c_cc_has_prefix(arg, "-MF") ||
        pk_c_cc_has_prefix(arg, "-MT") ||
        pk_c_cc_has_prefix(arg, "-MQ") ||
        pk_c_cc_has_prefix(arg, "-MJ") ||
        (pk_c_cc_has_prefix(arg, "-o") && arg[2] != '\0');
}

static int pk_c_cc_drop_without_value(const char *arg) {
    return strcmp(arg, "-c") == 0 ||
        strcmp(arg, "-S") == 0 ||
        strcmp(arg, "-E") == 0 ||
        strcmp(arg, "-M") == 0 ||
        strcmp(arg, "-MM") == 0 ||
        strcmp(arg, "-MD") == 0 ||
        strcmp(arg, "-MMD") == 0 ||
        strcmp(arg, "-MP") == 0 ||
        strcmp(arg, "-MG") == 0 ||
        strcmp(arg, "-pipe") == 0 ||
        strcmp(arg, "-save-temps") == 0;
}

static int pk_c_cc_drop_with_opacity(const char *arg) {
    return strcmp(arg, "-fplugin") == 0 ||
        pk_c_cc_has_prefix(arg, "-fplugin=") ||
        pk_c_cc_has_prefix(arg, "-fplugin-arg-");
}

static int pk_c_cc_preserve_with_next(const char *arg) {
    return strcmp(arg, "-I") == 0 ||
        strcmp(arg, "-D") == 0 ||
        strcmp(arg, "-U") == 0 ||
        strcmp(arg, "-include") == 0 ||
        strcmp(arg, "-imacros") == 0 ||
        strcmp(arg, "-isystem") == 0 ||
        strcmp(arg, "-idirafter") == 0 ||
        strcmp(arg, "-iquote") == 0 ||
        strcmp(arg, "-isysroot") == 0 ||
        strcmp(arg, "-x") == 0 ||
        strcmp(arg, "-std") == 0 ||
        strcmp(arg, "-target") == 0 ||
        strcmp(arg, "--target") == 0 ||
        strcmp(arg, "-arch") == 0;
}

static int pk_c_cc_preserve_attached(const char *arg) {
    return strcmp(arg, "-nostdinc") == 0 ||
        strcmp(arg, "-nostdinc++") == 0 ||
        strcmp(arg, "-pthread") == 0 ||
        pk_c_cc_has_prefix(arg, "-I") ||
        pk_c_cc_has_prefix(arg, "-D") ||
        pk_c_cc_has_prefix(arg, "-U") ||
        pk_c_cc_has_prefix(arg, "-isystem") ||
        pk_c_cc_has_prefix(arg, "-idirafter") ||
        pk_c_cc_has_prefix(arg, "-iquote") ||
        pk_c_cc_has_prefix(arg, "-isysroot") ||
        pk_c_cc_has_prefix(arg, "-std=") ||
        pk_c_cc_has_prefix(arg, "--std=") ||
        pk_c_cc_has_prefix(arg, "-target=") ||
        pk_c_cc_has_prefix(arg, "--target=") ||
        pk_c_cc_has_prefix(arg, "-m") ||
        pk_c_cc_has_prefix(arg, "-f") ||
        pk_c_cc_has_prefix(arg, "-W") ||
        pk_c_cc_has_prefix(arg, "-O") ||
        strcmp(arg, "-g") == 0 ||
        pk_c_cc_has_prefix(arg, "-g");
}

static int pk_c_cc_argv_has_language(const pk_c_cc_words *words, size_t start) {
    for (size_t i = start; i < words->len; i++) {
        if (strcmp(words->items[i], "-x") == 0 ||
            (pk_c_cc_has_prefix(words->items[i], "-x") && words->items[i][2] != '\0')) {
            return 1;
        }
    }
    return 0;
}

static int pk_c_cc_argv_has_std(const pk_c_cc_words *words, size_t start) {
    for (size_t i = start; i < words->len; i++) {
        if (strcmp(words->items[i], "-std") == 0 ||
            pk_c_cc_has_prefix(words->items[i], "-std=") ||
            pk_c_cc_has_prefix(words->items[i], "--std=")) {
            return 1;
        }
    }
    return 0;
}

static int pk_c_cc_add_default_args(
    pk_c_compile_context *context,
    const pk_c_cc_words *words,
    size_t start
) {
    if (!pk_c_cc_argv_has_language(words, start)) {
        if (pk_c_cc_add_arg(context, "-x") != 0 ||
            pk_c_cc_add_arg(context, "c") != 0) {
            return -1;
        }
    }
    if (!pk_c_cc_argv_has_std(words, start)) {
        if (pk_c_cc_add_arg(context, "-std=gnu11") != 0) {
            return -1;
        }
    }
    return 0;
}

static int pk_c_cc_arg_value_is_path(const char *arg) {
    return strcmp(arg, "-I") == 0 ||
        strcmp(arg, "-include") == 0 ||
        strcmp(arg, "-imacros") == 0 ||
        strcmp(arg, "-isystem") == 0 ||
        strcmp(arg, "-idirafter") == 0 ||
        strcmp(arg, "-iquote") == 0 ||
        strcmp(arg, "-isysroot") == 0;
}

static char *pk_c_cc_rebase_path_value(const char *base_dir, const char *value) {
    if (value == NULL) {
        return NULL;
    }
    if (base_dir == NULL || base_dir[0] == '\0' ||
        value[0] == '\0' || pk_c_cc_is_absolute_path(value) ||
        strcmp(value, "-") == 0) {
        return pk_c_cc_copy(value);
    }
    return pk_c_cc_join_path(base_dir, value);
}

static int pk_c_cc_add_rebased_path_value(
    pk_c_compile_context *context,
    const char *base_dir,
    const char *value
) {
    char *rebased = pk_c_cc_rebase_path_value(base_dir, value);
    int rc;

    if (rebased == NULL) {
        return -1;
    }
    rc = pk_c_cc_add_arg(context, rebased);
    free(rebased);
    return rc;
}

static const char *pk_c_cc_attached_path_prefix(const char *arg) {
    static const char *const prefixes[] = {
        "-I",
        "-isystem",
        "-idirafter",
        "-iquote",
        "-isysroot"
    };

    for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); i++) {
        size_t prefix_len = strlen(prefixes[i]);

        if (pk_c_cc_has_prefix(arg, prefixes[i]) && arg[prefix_len] != '\0') {
            return prefixes[i];
        }
    }
    return NULL;
}

static int pk_c_cc_add_rebased_attached_path_arg(
    pk_c_compile_context *context,
    const char *base_dir,
    const char *arg,
    const char *prefix
) {
    size_t prefix_len = strlen(prefix);
    char *rebased = pk_c_cc_rebase_path_value(base_dir, arg + prefix_len);
    char *attached;
    int rc;

    if (rebased == NULL) {
        return -1;
    }
    attached = malloc(prefix_len + strlen(rebased) + 1);
    if (attached == NULL) {
        free(rebased);
        return -1;
    }
    memcpy(attached, prefix, prefix_len);
    memcpy(attached + prefix_len, rebased, strlen(rebased) + 1);
    rc = pk_c_cc_add_arg(context, attached);
    free(attached);
    free(rebased);
    return rc;
}

static int pk_c_cc_add_opacity(
    pk_c_compile_context *context,
    const char *path,
    const char *kind,
    const char *reason
) {
    if (context->extraction_result == NULL) {
        context->extraction_result = pk_c_lift_result_new();
    }
    if (context->extraction_result == NULL) {
        return -1;
    }
    return pk_c_lift_result_add_opacity_entry(
        context->extraction_result,
        kind,
        path == NULL ? "" : path,
        1,
        1,
        reason,
        "compile-context");
}

static int pk_c_cc_ingest_args(
    pk_c_compile_context *context,
    const char *path,
    const pk_c_cc_words *words,
    const char *base_dir
) {
    size_t start = pk_c_cc_first_arg_index(words);

    if (pk_c_cc_add_default_args(context, words, start) != 0) {
        return -1;
    }
    for (size_t i = start; i < words->len; i++) {
        const char *arg = words->items[i];

        if (arg[0] == '\0') {
            continue;
        }
        if (pk_c_cc_drop_with_next(arg)) {
            if (i + 1 < words->len) {
                i++;
            }
            continue;
        }
        if (pk_c_cc_drop_with_opacity(arg)) {
            if (pk_c_cc_add_opacity(context, path, "compile-arg-dropped",
                "compile command option cannot be represented in a libclang parse context") != 0) {
                return -1;
            }
            if (strcmp(arg, "-fplugin") == 0 && i + 1 < words->len) {
                i++;
            }
            continue;
        }
        if (pk_c_cc_drop_without_value(arg) || pk_c_cc_drop_attached(arg) ||
            pk_c_cc_is_source_input(arg)) {
            continue;
        }
        if (pk_c_cc_preserve_with_next(arg)) {
            if (pk_c_cc_add_arg(context, arg) != 0) {
                return -1;
            }
            if (i + 1 < words->len) {
                if ((strcmp(arg, "-target") == 0 || strcmp(arg, "--target") == 0) &&
                    pk_c_cc_set_target(context, words->items[i + 1]) != 0) {
                    return -1;
                }
                if (pk_c_cc_arg_value_is_path(arg)) {
                    if (pk_c_cc_add_rebased_path_value(context, base_dir, words->items[i + 1]) != 0) {
                        return -1;
                    }
                } else if (pk_c_cc_add_arg(context, words->items[i + 1]) != 0) {
                    return -1;
                }
                i++;
            } else if (pk_c_cc_add_opacity(context, path, "compile-arg-missing-value",
                "compile command option was missing its required value") != 0) {
                return -1;
            }
            continue;
        }
        if (pk_c_cc_has_prefix(arg, "--target=")) {
            if (pk_c_cc_set_target(context, arg + strlen("--target=")) != 0 ||
                pk_c_cc_add_arg(context, arg) != 0) {
                return -1;
            }
            continue;
        }
        if (pk_c_cc_has_prefix(arg, "-target=")) {
            if (pk_c_cc_set_target(context, arg + strlen("-target=")) != 0 ||
                pk_c_cc_add_arg(context, arg) != 0) {
                return -1;
            }
            continue;
        }
        if (pk_c_cc_preserve_attached(arg)) {
            const char *path_prefix = pk_c_cc_attached_path_prefix(arg);

            if (path_prefix != NULL) {
                if (pk_c_cc_add_rebased_attached_path_arg(
                    context,
                    base_dir,
                    arg,
                    path_prefix) != 0) {
                    return -1;
                }
            } else if (pk_c_cc_add_arg(context, arg) != 0) {
                return -1;
            }
            continue;
        }
        if (arg[0] == '-') {
            if (pk_c_cc_add_arg(context, arg) != 0) {
                return -1;
            }
            continue;
        }
    }
    (void)path;
    return 0;
}

static pk_c_compile_context *pk_c_compile_context_from_command_with_base(
    const char *path,
    const char *command,
    const char *base_dir
) {
    pk_c_compile_context *context = pk_c_cc_context_new(command == NULL ? "" : command);
    pk_c_cc_words words = {0};
    int unclosed_quote = 0;

    if (context == NULL) {
        return NULL;
    }
    if (pk_c_cc_split_command(context->compile_command, &words, &unclosed_quote) != 0) {
        pk_c_compile_context_free(context);
        return NULL;
    }
    if (unclosed_quote &&
        pk_c_cc_add_opacity(context, path, "compile-command-unclosed-quote",
            "compile command contained an unclosed shell quote") != 0) {
        pk_c_cc_words_free(&words);
        pk_c_compile_context_free(context);
        return NULL;
    }
    if (pk_c_cc_ingest_args(context, path, &words, base_dir) != 0) {
        pk_c_cc_words_free(&words);
        pk_c_compile_context_free(context);
        return NULL;
    }
    pk_c_cc_words_free(&words);
    return context;
}

pk_c_compile_context *pk_c_compile_context_from_command(
    const char *path,
    const char *command
) {
    return pk_c_compile_context_from_command_with_base(path, command, NULL);
}

static char *pk_c_cc_join_words_for_provenance(const pk_c_cc_words *words) {
    size_t len = 0;
    char *out;
    char *dst;

    if (words == NULL || words->len == 0) {
        return pk_c_cc_copy("");
    }
    for (size_t i = 0; i < words->len; i++) {
        len += strlen(words->items[i]) + (i == 0 ? 0 : 1);
    }
    out = malloc(len + 1);
    if (out == NULL) {
        return NULL;
    }
    dst = out;
    for (size_t i = 0; i < words->len; i++) {
        size_t item_len = strlen(words->items[i]);

        if (i != 0) {
            *dst++ = ' ';
        }
        memcpy(dst, words->items[i], item_len);
        dst += item_len;
    }
    *dst = '\0';
    return out;
}

static pk_c_compile_context *pk_c_compile_context_from_words(
    const char *path,
    const pk_c_cc_words *words,
    const char *base_dir
) {
    char *command = pk_c_cc_join_words_for_provenance(words);
    pk_c_compile_context *context;

    if (command == NULL) {
        return NULL;
    }
    context = pk_c_cc_context_new(command);
    free(command);
    if (context == NULL) {
        return NULL;
    }
    if (pk_c_cc_ingest_args(context, path, words, base_dir) != 0) {
        pk_c_compile_context_free(context);
        return NULL;
    }
    return context;
}

pk_c_compile_context *pk_c_compile_context_from_kbuild_cmdline(
    const char *path,
    const char *cmdline
) {
    const char *command = cmdline == NULL ? "" : cmdline;
    const char *assignment = strstr(command, ":=");
    char *trimmed;
    pk_c_compile_context *context;

    if (assignment != NULL) {
        command = assignment + 2;
    }
    trimmed = pk_c_cc_trim_copy(command);
    if (trimmed == NULL) {
        return NULL;
    }
    context = pk_c_compile_context_from_command(path, trimmed);
    free(trimmed);
    if (context != NULL && assignment == NULL &&
        pk_c_cc_add_opacity(context, path, "kbuild-command-assignment-missing",
            "Kbuild command line did not contain a := assignment") != 0) {
        pk_c_compile_context_free(context);
        return NULL;
    }
    return context;
}

static int pk_c_cc_is_absolute_path(const char *path) {
    return path != NULL && path[0] == '/';
}

static char *pk_c_cc_join_path(const char *root, const char *path) {
    const char *base = root == NULL || root[0] == '\0' ? "." : root;
    size_t base_len = strlen(base);
    size_t path_len;
    char *joined;

    if (path == NULL) {
        path = "";
    }
    if (pk_c_cc_is_absolute_path(path)) {
        return pk_c_cc_copy(path);
    }
    path_len = strlen(path);
    joined = malloc(base_len + 1 + path_len + 1);
    if (joined == NULL) {
        return NULL;
    }
    memcpy(joined, base, base_len);
    joined[base_len] = '/';
    memcpy(joined + base_len + 1, path, path_len + 1);
    return joined;
}

static const char *pk_c_cc_relative_source(const char *root, const char *source_path) {
    size_t root_len;
    const char *source = source_path == NULL ? "" : source_path;

    while (source[0] == '.' && source[1] == '/') {
        source += 2;
    }
    if (root == NULL || !pk_c_cc_is_absolute_path(source)) {
        return source;
    }
    root_len = strlen(root);
    if (root_len == 0) {
        return source;
    }
    if (strncmp(source, root, root_len) == 0 && source[root_len] == '/') {
        source += root_len + 1;
        while (source[0] == '.' && source[1] == '/') {
            source += 2;
        }
        return source;
    }
    return source;
}

static char *pk_c_cc_kbuild_cmd_path(const char *root, const char *source_path) {
    const char *relative = pk_c_cc_relative_source(root, source_path);
    const char *slash = strrchr(relative, '/');
    const char *base = slash == NULL ? relative : slash + 1;
    size_t dir_len = slash == NULL ? 0 : (size_t)(slash - relative);
    size_t base_len = strlen(base);
    size_t stem_len;
    size_t rel_len;
    char *rel_cmd;
    char *joined;
    char *dst;

    if (base_len < 3 || strcmp(base + base_len - 2, ".c") != 0) {
        return NULL;
    }
    stem_len = base_len - 2;
    rel_len = dir_len + (dir_len == 0 ? 0 : 1) + 1 + stem_len + strlen(".o.cmd");
    rel_cmd = malloc(rel_len + 1);
    if (rel_cmd == NULL) {
        return NULL;
    }
    dst = rel_cmd;
    if (dir_len != 0) {
        memcpy(dst, relative, dir_len);
        dst += dir_len;
        *dst++ = '/';
    }
    *dst++ = '.';
    memcpy(dst, base, stem_len);
    dst += stem_len;
    memcpy(dst, ".o.cmd", strlen(".o.cmd") + 1);

    joined = pk_c_cc_join_path(root, rel_cmd);
    free(rel_cmd);
    return joined;
}

static char *pk_c_cc_read_file(const char *path) {
    FILE *f = fopen(path, "rb");
    long len;
    char *data;

    if (f == NULL) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    len = ftell(f);
    if (len < 0) {
        fclose(f);
        return NULL;
    }
    if (fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return NULL;
    }
    data = malloc((size_t)len + 1);
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

static int pk_c_cc_path_has_suffix(const char *path, const char *suffix) {
    size_t path_len;
    size_t suffix_len;

    if (path == NULL || suffix == NULL) {
        return 0;
    }
    path_len = strlen(path);
    suffix_len = strlen(suffix);
    return path_len > suffix_len &&
        path[path_len - suffix_len - 1] == '/' &&
        strcmp(path + path_len - suffix_len, suffix) == 0;
}

static int pk_c_cc_compile_entry_matches(
    const char *root,
    const char *source_path,
    const char *directory,
    const char *file
) {
    const char *source = pk_c_cc_relative_source(root, source_path);
    char *root_source = NULL;
    char *directory_file = NULL;
    int match = 0;

    if (file == NULL || source == NULL) {
        return 0;
    }
    if (strcmp(file, source) == 0 || pk_c_cc_path_has_suffix(file, source)) {
        return 1;
    }
    root_source = pk_c_cc_join_path(root, source);
    if (root_source != NULL && strcmp(file, root_source) == 0) {
        match = 1;
    }
    if (!match && directory != NULL && !pk_c_cc_is_absolute_path(file)) {
        directory_file = pk_c_cc_join_path(directory, file);
        if (directory_file != NULL &&
            (strcmp(directory_file, source) == 0 ||
                (root_source != NULL && strcmp(directory_file, root_source) == 0) ||
                pk_c_cc_path_has_suffix(directory_file, source))) {
            match = 1;
        }
    }
    free(root_source);
    free(directory_file);
    return match;
}

static char *pk_c_cc_json_decode_string_at(const char *quote, const char **end_out) {
    const char *p;
    char *out;
    size_t cap;
    size_t len = 0;

    if (quote == NULL || *quote != '"') {
        return NULL;
    }
    cap = strlen(quote) + 1;
    out = malloc(cap);
    if (out == NULL) {
        return NULL;
    }
    p = quote + 1;
    while (*p != '\0' && *p != '"') {
        if (*p == '\\') {
            p++;
            switch (*p) {
            case '"':
            case '\\':
            case '/':
                out[len++] = *p++;
                break;
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
            default:
                free(out);
                return NULL;
            }
        } else {
            out[len++] = *p++;
        }
    }
    if (*p != '"') {
        free(out);
        return NULL;
    }
    out[len] = '\0';
    if (end_out != NULL) {
        *end_out = p + 1;
    }
    return out;
}

static char *pk_c_cc_json_string_field(const char *object, const char *field) {
    char needle[128];
    const char *p;
    int written;

    written = snprintf(needle, sizeof(needle), "\"%s\"", field);
    if (written < 0 || (size_t)written >= sizeof(needle)) {
        return NULL;
    }
    p = strstr(object, needle);
    if (p == NULL) {
        return NULL;
    }
    p += strlen(needle);
    while (isspace((unsigned char)*p)) {
        p++;
    }
    if (*p != ':') {
        return NULL;
    }
    p++;
    while (isspace((unsigned char)*p)) {
        p++;
    }
    if (*p != '"') {
        return NULL;
    }
    return pk_c_cc_json_decode_string_at(p, NULL);
}

static int pk_c_cc_json_string_array_field(
    const char *object,
    const char *field,
    pk_c_cc_words *words
) {
    char needle[128];
    const char *p;
    int written;

    memset(words, 0, sizeof(*words));
    written = snprintf(needle, sizeof(needle), "\"%s\"", field);
    if (written < 0 || (size_t)written >= sizeof(needle)) {
        return -1;
    }
    p = strstr(object, needle);
    if (p == NULL) {
        return 0;
    }
    p += strlen(needle);
    while (isspace((unsigned char)*p)) {
        p++;
    }
    if (*p != ':') {
        return -1;
    }
    p++;
    while (isspace((unsigned char)*p)) {
        p++;
    }
    if (*p != '[') {
        return -1;
    }
    p++;
    while (*p != '\0') {
        char *item;

        while (isspace((unsigned char)*p)) {
            p++;
        }
        if (*p == ']') {
            return 0;
        }
        if (*p != '"') {
            pk_c_cc_words_free(words);
            return -1;
        }
        item = pk_c_cc_json_decode_string_at(p, &p);
        if (item == NULL) {
            pk_c_cc_words_free(words);
            return -1;
        }
        if (pk_c_cc_words_add_owned(words, item) != 0) {
            pk_c_cc_words_free(words);
            return -1;
        }
        while (isspace((unsigned char)*p)) {
            p++;
        }
        if (*p == ',') {
            p++;
            continue;
        }
        if (*p == ']') {
            return 0;
        }
        pk_c_cc_words_free(words);
        return -1;
    }
    pk_c_cc_words_free(words);
    return -1;
}

static const char *pk_c_cc_json_object_end(const char *start) {
    const char *p = start;
    int depth = 0;
    int in_string = 0;
    int escaped = 0;

    while (*p != '\0') {
        if (in_string) {
            if (escaped) {
                escaped = 0;
            } else if (*p == '\\') {
                escaped = 1;
            } else if (*p == '"') {
                in_string = 0;
            }
        } else if (*p == '"') {
            in_string = 1;
        } else if (*p == '{') {
            depth++;
        } else if (*p == '}') {
            depth--;
            if (depth == 0) {
                return p + 1;
            }
        }
        p++;
    }
    return NULL;
}

static pk_c_compile_context *pk_c_cc_resolve_compile_commands(
    const char *root,
    const char *source_path
) {
    char *path = pk_c_cc_join_path(root, "compile_commands.json");
    char *data;
    const char *cursor;
    pk_c_compile_context *context = NULL;

    if (path == NULL) {
        return NULL;
    }
    data = pk_c_cc_read_file(path);
    free(path);
    if (data == NULL) {
        return NULL;
    }
    cursor = data;
    while ((cursor = strchr(cursor, '{')) != NULL) {
        const char *end = pk_c_cc_json_object_end(cursor);
        char *object;
        char *directory;
        char *file;
        char *command;
        pk_c_cc_words arguments;

        if (end == NULL) {
            break;
        }
        object = pk_c_cc_copy_n(cursor, (size_t)(end - cursor));
        if (object == NULL) {
            break;
        }
        directory = pk_c_cc_json_string_field(object, "directory");
        file = pk_c_cc_json_string_field(object, "file");
        command = pk_c_cc_json_string_field(object, "command");
        if (pk_c_cc_json_string_array_field(object, "arguments", &arguments) != 0) {
            memset(&arguments, 0, sizeof(arguments));
        }
        if (pk_c_cc_compile_entry_matches(root, source_path, directory, file)) {
            if (command != NULL) {
                context = pk_c_compile_context_from_command_with_base(
                    source_path,
                    command,
                    directory);
            } else if (arguments.len > 0) {
                context = pk_c_compile_context_from_words(source_path, &arguments, directory);
            }
            free(directory);
            free(file);
            free(command);
            pk_c_cc_words_free(&arguments);
            free(object);
            break;
        }
        free(directory);
        free(file);
        free(command);
        pk_c_cc_words_free(&arguments);
        free(object);
        cursor = end;
    }
    free(data);
    return context;
}

static pk_c_compile_context *pk_c_cc_resolve_kbuild_cmd(
    const char *root,
    const char *source_path
) {
    char *path = pk_c_cc_kbuild_cmd_path(root, source_path);
    char *data;
    pk_c_compile_context *context;

    if (path == NULL) {
        return NULL;
    }
    data = pk_c_cc_read_file(path);
    free(path);
    if (data == NULL) {
        return NULL;
    }
    context = pk_c_compile_context_from_kbuild_cmdline(source_path, data);
    free(data);
    return context;
}

pk_c_compile_context *pk_c_compile_context_resolve_kernel(
    const char *workspace_root,
    const char *source_path
) {
    const char *root = workspace_root == NULL || workspace_root[0] == '\0' ? "." : workspace_root;
    const char *source = source_path == NULL ? "" : source_path;
    pk_c_compile_context *context = pk_c_cc_resolve_compile_commands(root, source);

    if (context != NULL) {
        return context;
    }
    context = pk_c_cc_resolve_kbuild_cmd(root, source);
    if (context != NULL) {
        return context;
    }
    context = pk_c_compile_context_from_command(source, "");
    if (context != NULL &&
        pk_c_cc_add_opacity(context, source, "kernel-compile-context-missing",
            "no compile_commands.json entry or Kbuild .cmd file was found for this source") != 0) {
        pk_c_compile_context_free(context);
        return NULL;
    }
    return context;
}

void pk_c_compile_context_configure_parse_options(
    const pk_c_compile_context *context,
    pk_c_parse_backend backend,
    pk_c_parse_options *options
) {
    if (options == NULL) {
        return;
    }
    memset(options, 0, sizeof(*options));
    options->backend = backend;
    if (context == NULL) {
        return;
    }
    options->clang_args = (const char *const *)context->clang_args;
    options->n_clang_args = context->n_clang_args;
    options->compile_command = context->compile_command;
    options->target_triple = context->target_triple;
}

void pk_c_compile_context_free(pk_c_compile_context *context) {
    if (context == NULL) {
        return;
    }
    for (size_t i = 0; i < context->n_clang_args; i++) {
        free(context->clang_args[i]);
    }
    free(context->clang_args);
    free(context->compile_command);
    free(context->target_triple);
    pk_c_lift_result_free(context->extraction_result);
    free(context);
}
