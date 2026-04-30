// SPDX-License-Identifier: Apache-2.0
//
// Convert provekit-ir-symbolic's Formula/Term/Sort tree into the
// canonicalizer's Value DOM, so the IR can be embedded as the
// `evidence.body.irFormula` field of a property memento and JCS-
// encoded with the rest of the envelope.
//
// The Value-tree shape MUST match the IR-JSON encoding the TS kit
// emits (see protocol/specs/2026-04-30-ir-formal-grammar.md). Both
// kits hash to byte-equal envelope CIDs when given byte-equal logical
// inputs.

#pragma once

#include "../canonicalizer/value.hpp"
#include "provekit/ir.hpp"

namespace provekit::claim_envelope {

::provekit::canonicalizer::ValuePtr sort_to_value(const ::provekit::ir::Sort& sort);
::provekit::canonicalizer::ValuePtr term_to_value(const ::provekit::ir::Term& term);
::provekit::canonicalizer::ValuePtr formula_to_value(const ::provekit::ir::Formula& formula);

}  // namespace provekit::claim_envelope
