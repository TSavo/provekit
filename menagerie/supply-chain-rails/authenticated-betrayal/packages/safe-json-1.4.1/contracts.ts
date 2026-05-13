// SPDX-License-Identifier: Apache-2.0

/**
 * @provekit .target supply-chain:parse.deterministic
 * @provekit .post {"kind":"atomic","name":"parse.deterministic","args":[]}
 */
export function parseDeterministic(out: unknown): unknown {
  return out;
}

/**
 * @provekit .target supply-chain:parse.no-network-effect
 * @provekit .post {"kind":"atomic","name":"parse.no-network-effect","args":[]}
 */
export function parseNoNetworkEffect(out: unknown): unknown {
  return out;
}

/**
 * @provekit .target supply-chain:package.no-install-side-effect
 * @provekit .post {"kind":"atomic","name":"package.no-install-side-effect","args":[]}
 */
export function packageNoInstallSideEffect(out: unknown): unknown {
  return out;
}

/**
 * @provekit .target supply-chain:runtime.no-env-secret-read
 * @provekit .post {"kind":"atomic","name":"runtime.no-env-secret-read","args":[]}
 */
export function runtimeNoEnvSecretRead(out: unknown): unknown {
  return out;
}
