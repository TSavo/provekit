// SPDX-License-Identifier: Apache-2.0
//
// MintedMemento: the lifter's per-statement output. The mint pipeline
// (NOT this lifter) consumes these to compute CIDs and emit signed
// .proof envelopes. The lifter only:
//   1. Produces the IR-JSON byte sequence (`IrJson`).
//   2. Records which variable this memento describes (`OutBinding`).
//   3. Records logical predecessors by name (`InputBindings`); the
//      mint pipeline later resolves those to CIDs once each predecessor
//      has been hashed. This is the chain-DAG primitive.
//
// Why not record CIDs here directly? Because computing a CID requires
// JCS-canonicalizing the WRAPPED memento envelope (with bindingHash,
// propertyHash, signature stripped, etc.) -- that is a separate concern
// owned by Provekit.ClaimEnvelope. The lifter's job is upstream of that.

namespace Provekit.Lift.Linq;

public sealed record MintedMemento(
    string Name,
    string OutBinding,
    IReadOnlyList<string> InputBindings,
    ContractDecl Contract,
    string IrJson,
    string SourceSpan,
    LinqBodySource BodySource);
