# std-core-showcase

This showcase proves a sound, narrow slice of Rust `core` using the standard
library's own tests and no source changes.

Claimed slice:

- Source: the pinned Rust `1.96.0` toolchain's `rust-src` component, under
  `library/coretests`. The run script installs `rust-src` for that pinned
  toolchain and does not use CI's active default std source.
- Spec: selected scalar direct call-result `assert_eq!` rows from std/core own
  tests plus direct type-reflection comparisons:
  - `coretests/tests/cmp.rs`: integer rows for
    `cmp::{min_by,max_by,min_by_key,max_by_key}`.
  - `coretests/tests/mem.rs`: generic type-arg-keyed rows for
    `size_of::<T>()` and `align_of::<T>()` from the non-cfg
    `size_of_basic` / `align_of_basic` vendor tests plus the active
    pinned-target pointer-width cfg tests.
  - `coretests/tests/intrinsics.rs`: direct `TypeId::of::<T>()` equality and
    inequality rows from `test_typeid_sized_types` and
    `test_typeid_unsized_types`.
  - `coretests/tests/time.rs`: finite decimal float rows and width-known
    NaN refinement predicate rows for `Duration::div_duration_f{32,64}`.
  - `coretests/tests/fmt/mod.rs`: exact string rows for `to_string()`.
  - `coretests/tests/alloc.rs` and `coretests/tests/ops.rs`: pure
    method-chain predicate rows such as `layout.align_to(3).is_err()` and
    `(1u32..5).contains(&1u32)`.
  - `coretests/tests/atomic.rs`: stable-key compound RHS rows where the
    asserted value is a closed bitwise expression, such as
    `x.load(SeqCst) == 0xf731 & 0x137f`.
  - `coretests/tests/iter/range.rs`: stable-key method-chain call-result rows
    whose expected values include exact literal array or tuple identities, such
    as `collect::<Vec<_>>() == [0, 1, 2, 3, 4]` and
    `size_hint() == (0, Some(100))`.
  - `coretests/tests/array.rs`: expression-only `const { expr }` wrappers
    around stable call-result rows from `const_array_ops`, including
    `std::array::from_fn::<_, 5, _>(doubler)` and
    `[5, 6, 1, 2].map(doubler)`.
  - `coretests/tests/option.rs::test_and`: nullary and variant constructor
    equality rows for immutable `Option` values, lifted as location-keyed
    operator-dispatch claims.
  - `coretests/tests/result.rs::result_try_trait_v2_branch`: nested variant
    constructor equality rows such as `Break(Err(4))`, lifted as
    location-keyed operator-dispatch claims.
  - `coretests/tests/cmp.rs::cmp_default`: user-type operator dispatch for
    `Int`, `RevInt`, and `Fool` comparisons, lifted as uninterpreted operator
    call results.
- Proof axis: `sugar mint` + `sugar verify` through `rust-test-assertions`
  emits `#euf#` call-result consistency rows, TypeId consistency rows, and the
  `cmp_default`, `option::test_and`, and `result_try_trait_v2_branch`
  operator-dispatch rows, and every claimed row discharges.
- Witness axis: the exact std vendor tests rerun with `cargo test --test
  coretests ... -- --exact`.

Named gaps toward full `coretests` coverage:

- Generic identity: type arguments are carried in the `#euf#` key for direct
  generic call results. The showcase claims the non-cfg `size_of_basic` and
  `align_of_basic` rows plus the active pointer-width cfg rows for the
  explicitly pinned target.
- Operator dispatch typing: comparisons whose user-typed operand is built by a
  lowercase-named function (not an uppercase constructor) still take the FOL path;
  on the consistency axis the failure direction is conservative (false-refusal,
  never falsePass). Named residual until operand typing is kit-resolvable.
- Macros: bounded ASCII `assert_all!` / `assert_none!` expansion lives in the
  lifter, but this scalar std-core showcase does not claim broader macro
  surfaces.
- Float refinements: direct width-known `is_nan()` rows over
  `Duration::div_duration_f{32,64}` are claimed. Infinity constants,
  ordered comparisons, signed zero, width-unknown NaN predicates, and
  approximate/tolerance assertions remain residual. Exact finite
  exponent-form literals now normalize to Real constants when they appear in
  the scalar lifter surface.
- Strings/chars: exact string equality is claimed here where it is a direct
  call-result value. Richer point-wise string predicates and ASCII char
  predicates are covered by the sibling `std-core-string-predicates`
  showcase; Unicode `char::is_alphabetic` remains a named residual.
- Method chains: pure immutable method-chain predicates and exact literal
  array/tuple value rows with stable receiver identity are claimed. Stateful
  chains or tests that reassign a receiver name remain out of scope until the
  key can carry temporal identity.
- CFG-sensitive tests: only assertions whose `#[cfg]` predicates are active
  under the explicitly pinned Rust target are claimed. Inactive and ambiguous
  cfg predicates remain named residuals.
- Type identity: direct `TypeId::of::<T>()` comparisons from current
  `coretests/tests/intrinsics.rs` are claimed. Dynamic `Any::is::<T>()`
  predicates remain in the no-scalar assertion bucket until that predicate form
  is lifted explicitly.
- Complex terms: closed bitwise-expression RHS terms, exact literal array/tuple
  value identities, and expression-only `const { expr }` wrappers are claimed
  where the call-result key is stable. Nullary/variant constructor expected
  values are claimed through operator dispatch on immutable inputs, such as
  `option::test_and`, and one-level nested variant constructors are claimed
  where the inner constructor is an exact value term, such as
  `result_try_trait_v2_branch`; arrays and tuples are opaque exact values in
  ProofIR, and aggregate literals containing non-literal elements are
  conservatively skipped. Direct aggregate constructor reasoning beyond those
  bounded operator-dispatch shapes stays residual. Const blocks with statements,
  control flow, or unsupported inner terms stay residual. Nested calls with
  non-value callees, stateful method chains, non-direct-call results, and
  unsupported expression forms stay out of the claimed slice.

The run script requires representative integer, generic type-arg-keyed, active
cfg pointer-width, TypeId comparison, finite-float, width-known NaN refinement,
string, pure method-chain predicate, stable-key compound RHS rows, literal
array/tuple exact-value rows, expression-only const-block rows, the
`option::test_and` constructor operator-dispatch row, the
`result_try_trait_v2_branch` nested constructor operator-dispatch row, and the
`cmp_default` user-type operator row, and rejects any non-discharged claimed
row. It is intentionally not a full-`std` claim.

## Grounded Full-Coretests Gap Census

This census is from the full `library/coretests` lift/verify run on
`rustc 1.96.0 (ac68faa20 2026-05-25)`. The count unit is one non-lift diagnostic
item plus one emitted row that did not discharge. It is not a per-assertion
count; it is the current engineering backlog shape for reaching all of
`coretests`.

Original pre-generics total: 1,146 gap items = 1,119 lift diagnostics + 27
non-discharged emitted rows. The generics slice closed the 30 direct generic
call-result identity items by carrying type arguments in the `#euf#` key. The
method-chain slice closed 17 pure method-chain predicate items. The pinned
target cfg slice closes 4 active pointer-width `mem.rs` rows on a 64-bit target.
This compound term slice claims 18 additional stable-key atomic rows, 8 of
which close operator-expression RHS complex terms. The TypeId comparison slice
closes 2 current `intrinsics.rs` diagnostic items. The operator-dispatch slice
closes the pre-existing `cmp_default` over-refusal. This literal aggregate
method-chain slice closes 13 additional stable-key `iter/range.rs` rows. This
scoped local-function identity. The first constructor-dispatch slice closes 8
`option.rs::test_and` nullary/variant constructor rows. This nested-constructor
slice closes 6 `result.rs::result_try_trait_v2_branch` nested variant
constructor rows. This float-refinement slice closes 2 width-known `time.rs` NaN
rows, moving the combined std-core showcase count from `137+2+1` to `139+2+1`; a
fresh full lift-only census for that lever emitted 1,703 IR declarations and
1,031 lift diagnostics. The current known backlog is 1,045 items for that
target.

| Gap type | Count | Representative std test/assertion |
| --- | ---: | --- |
| Generics | 30 | Closed for direct generic call-result identity in this slice by carrying type args in the `#euf#` key; active cfg-sensitive pointer-width variants are tracked under CFG-sensitive. |
| Macros | 40 | Broad macro surfaces remain residual here; bounded ASCII `assert_all!` / `assert_none!` expansion is handled by the lifter but is outside this showcase's claimed scalar slice. |
| Floats | 18 prior full lift+verify census, with 2 NaN showcase rows now closed | `tests/num/const_from.rs`: `assert_eq!(FROM_F64, 42f64)` remains outside the exact finite direct call-result slice. Other residual examples include infinity constants/equality, ordered comparisons, signed zero, width-unknown parsed NaN predicates, and aggregate literals containing NaN. |
| Strings/chars | 183 | `tests/alloc.rs::layout_debug_shows_log2_of_alignment`: expected string literal for `Layout` debug output; not a direct call-result equality row. |
| CFG-sensitive | 61 | Residual after 4 closed: active `tests/mem.rs` `#[cfg(target_pointer_width = "64")]` rows for `size_of::<usize>()`, `size_of::<*const usize>()`, `align_of::<usize>()`, and `align_of::<*const usize>()` are claimed when the pinned target cfg facts say `target_pointer_width = "64"`; inactive widths and other cfg-sensitive tests remain residuals. |
| Complex terms | 413 | Residual after the complex-term, TypeId, literal aggregate method-chain, expression-only const-block, and constructor-dispatch slices: current `tests/intrinsics.rs::{test_typeid_sized_types,test_typeid_unsized_types}` direct `TypeId::of::<T>()` comparison rows are claimed, 13 stable `iter/range.rs` method-chain rows lift with opaque exact array/tuple literal identities, 2 `array.rs::const_array_ops` rows lift through expression-only const blocks with scoped local-function identity, and 8 `option.rs::test_and` constructor rows plus 6 `result.rs::result_try_trait_v2_branch` nested constructor rows lift through operator dispatch. Remaining term shapes are outside these bounded slices or belong to expression-structure work. |
| Other | 331 | `tests/alloc.rs::layout_round_up_to_align_edge_cases`: no liftable scalar assertion under the current surface. |

### Complex-Term Decomposition

The current complex-term residual sub-shapes include:

| Sub-shape | Count | Representative std test/assertion |
| --- | ---: | --- |
| Method-chain predicates | 90 | Closed 17 pure rows in this slice; remaining examples include reassigned receiver cases such as `tests/ops.rs::test_range_bounds`: `r.contains(&0)` after `r` is rebound. |
| References, derefs, casts, unsafe blocks | 81 | `tests/array.rs::array_from_mut`: `assert_eq!(&value, "Hello World!")`. |
| Method chains returning compared values | 54 | Closed 13 stable `tests/iter/range.rs::test_range` rows and the expression-only const-block method-chain row `tests/array.rs::const_array_ops`: `assert_eq!(const { [5, 6, 1, 2].map(doubler) }, [10, 12, 2, 4])`; residual examples include `tests/array.rs::iterator_nth`: `assert_eq!(IntoIterator::into_iter(v.clone()).nth(i).unwrap(), v[i])`. |
| Residual unsupported term shapes | 59 | Closed current direct `TypeId::of::<T>()` comparison rows from `tests/intrinsics.rs`; remaining term-shape residuals exclude the stale `tests/any.rs::any_fixed_vec` TypeId example, which is now an `Any::is::<T>()` predicate in the pinned source. |
| Operator / expression RHS | 48 | This slice closes stable-key atomic bitwise RHS rows such as `tests/atomic.rs::uint_and`: `assert_eq!(x.load(SeqCst), 0xf731 & 0x137f)`; residual rows include stateful/repeated receiver and pointer arithmetic forms needing temporal identity. |
| Array, slice, and tuple literals | 47 | Exact literal array/tuple identities are now claimed only when they sit on stable call-result rows. This slice also closes the expression-only const-block free-call row `tests/array.rs::const_array_ops`: `assert_eq!(const { std::array::from_fn::<_, 5, _>(doubler) }, [0, 2, 4, 6, 8])`. Direct aggregate comparisons and aggregate literals with non-literal elements remain residual, for example `tests/array.rs::array_from_ref`: `assert_eq!(&[*VALUE], ARR)`. |
| Boolean operators / non-equality predicates | 25 | `tests/array.rs::array_mixed_equality_integers`: `assert!(array3 != slice3b)`. |
| Nested calls / constructors | 6 | Closed 8 immutable `Option` constructor-dispatch rows from `tests/option.rs::test_and` and 6 nested variant constructor rows from `tests/result.rs::result_try_trait_v2_branch`; residual examples include `tests/async_iter/mod.rs::into_async_iter`: `assert_eq!(..., Poll::Ready(Some(0)))`, where the polled receiver is mutable and stateful. |
| Boolean predicate residual | 13 | `tests/array.rs::array_from_ref`: `assert!(core::ptr::eq(VALUE, &ARR[0]))`. |

### Other / No-Liftable Decomposition

The 331 no-liftable bucket breaks down as:

| Sub-shape | Count | Representative std test/assertion |
| --- | ---: | --- |
| Text / format behavior not in direct call-result form | 60 | `tests/ascii.rs::test_is_ascii`: `assert!("banana\\0\\u{7F}".chars().all(|c| c.is_ascii()))`. |
| Data-structure setup / property tests | 58 | `tests/alloc.rs::layout_round_up_to_align_edge_cases`: setup-heavy layout arithmetic with no scalar call-result equality row. |
| Unsafe, pointer, memory, and atomic behavior | 46 | `tests/atomic.rs::bool_`: `compare_exchange` result rows over `Ok`/`Err` values. |
| Iterator / range behavior and setup-only tests | 40 | `tests/async_iter/mod.rs::into_async_iter`: pinned async iterator polling sequence. |
| Numeric property loops / tables | 39 | `tests/num/bignum.rs::test_from_u64_overflow`: table-driven bignum behavior. |
| Miscellaneous no-scalar assertion tests | 38 | `tests/any.rs::any_referenced`: type identity predicates via `Any::is::<T>()`. |
| Protocol / runtime behavior | 27 | `tests/bool.rs::test_bool_not`: boolean runtime behavior outside direct call-result equality. |
| Panic / `should_panic` tests | 18 | `tests/array.rs::array_map_drops_unmapped_elements_on_panic`: panic/drop behavior, not scalar equality. |
| Macro-only / setup-only tests | 5 | `tests/macros.rs::assert_escape`: macro behavior with no liftable row under this surface. |
