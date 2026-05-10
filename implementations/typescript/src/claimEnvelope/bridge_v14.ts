// SPDX-License-Identifier: Apache-2.0
//
// v1.4 BridgeDeclaration: layered envelope/header/body, tagged-union target.
//
// Per protocol/specs/2026-05-03-bridge-target-dimensionality.md §1.R1-R6.
// Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
//
// Coexists with existing v1.1 BridgeDeclaration.

/** Tagged-union target per spec §1.R1. */
export type BridgeTarget =
  | { kind: "contract"; cid: string }
  | { kind: "contractSet"; cid: string };

/** v1.4 bridge mint inputs. Metadata fields are optional (omit when undefined). */
export interface MintBridgeV14Args {
  // header (7 canonical fields per §1.R3)
  name: string;
  sourceSymbol: string;
  sourceLayer: string;
  sourceContractCid: string;
  target: BridgeTarget;

  // metadata (omit when undefined)
  targetWitnessCid?: string;
  targetBinaryCid?: string;
  targetLayer?: string;
  targetContractSetCid?: string;
  producedBy?: string;
  producedAt?: string;

  // envelope
  declaredAt: string;
}

/** Result of mint_bridge_v14. */
export interface MintedV14 {
  canonicalBytes: Uint8Array;
  cid: string;
}
