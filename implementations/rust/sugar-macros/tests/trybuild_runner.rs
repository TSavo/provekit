// SPDX-License-Identifier: Apache-2.0
//
// trybuild driver: compiles every fixture under `tests/pass/` and
// asserts the listed `tests/compile_fail/` fixtures fail with the
// stored .stderr. Run with:
//
//   cargo test -p sugar-macros --test trybuild_runner
//
// Set `TRYBUILD=overwrite` to regenerate the .stderr snapshots.

#[test]
fn trybuild_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/pass/basic_contract.rs");
}

#[test]
fn trybuild_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/empty_contract.rs");
}
