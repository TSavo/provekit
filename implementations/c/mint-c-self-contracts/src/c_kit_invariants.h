/* SPDX-License-Identifier: Apache-2.0 */
#ifndef MCSC_C_KIT_INVARIANTS_H
#define MCSC_C_KIT_INVARIANTS_H

#include "slab.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Author every C-kit slab and return them as an mcsc_slab_list. Returns
 * NULL on allocation failure. Caller owns the returned list and must
 * free it with mcsc_slab_list_free. */
mcsc_slab_list *mcsc_author_all(void);

#ifdef __cplusplus
}
#endif

#endif /* MCSC_C_KIT_INVARIANTS_H */
