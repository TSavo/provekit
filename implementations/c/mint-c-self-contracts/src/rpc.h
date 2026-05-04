/* SPDX-License-Identifier: Apache-2.0 */
#ifndef MCSC_RPC_H
#define MCSC_RPC_H

#ifdef __cplusplus
extern "C" {
#endif

/* Run the lift-plugin protocol RPC server on stdin/stdout. Returns
 * process exit code (0 on graceful shutdown). */
int mcsc_run_rpc(void);

#ifdef __cplusplus
}
#endif

#endif /* MCSC_RPC_H */
