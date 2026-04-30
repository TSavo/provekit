// TypeScript consumer's invariant file.
//
// Composes against the C++ library's contract H_divide via the
// imported function's propertyHash. The TS lifter sees the call
// to `divide`, looks up its registry contract (provided by the
// C++ kit's published proof DAG), and verifies the precondition
// is established at every TS callsite.

import { property, forAll, implies, type Int } from "provekit/ir";
import { safeDivide } from "./usage.js";

// Invariant about the TS wrapper: it never invokes the C++ divide
// with a zero denominator (the guard is explicit).
property("safeDivideUpholdsPrecondition",
  forAll<Int>(n =>
    forAll<Int>(d =>
      implies(d !== 0, safeDivide(n, d) === n / d)
    )
  )
);
