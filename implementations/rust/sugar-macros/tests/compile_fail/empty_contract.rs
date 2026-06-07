// SPDX-License-Identifier: Apache-2.0
//
// trybuild COMPILE-FAIL fixture: empty contract is rejected at the
// attribute parse stage. The macro emits a compile_error! with a
// stable message; trybuild snapshots the .stderr output.

use sugar_macros::contract;

#[contract()]
fn must_have_at_least_one() -> i64 {
    0
}

fn main() {}
