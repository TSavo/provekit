// SPDX-License-Identifier: Apache-2.0
//
// Unnamed-cluster example.
//
// Two functions below share a structural shape the smoke-test driver's
// seed catalog does not yet have a name for: a three-way bound applied
// to a single input (a saturating clamp). Pass 1 surfaces the cluster
// as UNNAMED-CONCEPT-N. Pass 1.5 (a simulated human edit on the
// rewritten output) writes a name into the annotation. Pass 2 picks
// up the name and binds both sites to the human-supplied concept.
//
// The driver's clustering merges these two functions because both
// classify to the same algebra shape; their shape-CIDs differ slightly
// because of variable-name and order differences, but the catalog's
// classifier groups them.

pub fn clamp_score(score: i64, lo: i64, hi: i64) -> i64 {
    if score < lo {
        lo
    } else if score > hi {
        hi
    } else {
        score
    }
}

pub fn clamp_pressure(p: i64, min: i64, max: i64) -> i64 {
    if p < min {
        min
    } else if p > max {
        max
    } else {
        p
    }
}
