/* SPDX-License-Identifier: Apache-2.0 */

#include <stdbool.h>
#include <stdint.h>

typedef struct {
    bool overflow;
    uint8_t value;
} overflow_add_u8_result;

/* provekit:contract overflow_add_u8.postcondition */
overflow_add_u8_result overflow_add_u8(uint8_t a, uint8_t b) {
    uint16_t wide = (uint16_t)a + (uint16_t)b;
    return (overflow_add_u8_result){ .overflow = false, .value = (uint8_t)wide };
}
