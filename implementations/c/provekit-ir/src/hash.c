/* SPDX-License-Identifier: Apache-2.0 */
/*
 * Hash delegation: shells out to Python blake3 module.
 * Native C BLAKE3 planned for v1.2.
 */

#include "provekit/ir.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

char *pk_hash_jcs(const char *jcs_string) {
    if (!jcs_string) return NULL;

    /* Write JCS bytes to a named temp file. */
    char path[] = "/tmp/pk_hash_XXXXXX";
    int fd = mkstemp(path);
    if (fd < 0) return NULL;

    size_t len = strlen(jcs_string);
    if (write(fd, jcs_string, len) != (ssize_t)len) {
        close(fd);
        unlink(path);
        return NULL;
    }
    close(fd);

    char cmd[1024];
    snprintf(cmd, sizeof(cmd),
        "python3 -c \"import blake3; data=open('%s','rb').read(); print('blake3-512:' + blake3.blake3(data).digest(length=64).hex())\"",
        path);

    FILE *pipe = popen(cmd, "r");
    if (!pipe) {
        unlink(path);
        return NULL;
    }

    char result[256];
    if (fgets(result, sizeof(result), pipe) == NULL) {
        pclose(pipe);
        unlink(path);
        return NULL;
    }
    pclose(pipe);
    unlink(path);

    /* Strip trailing newline. */
    size_t rlen = strlen(result);
    if (rlen > 0 && result[rlen - 1] == '\n') result[rlen - 1] = '\0';

    return strdup(result);
}
