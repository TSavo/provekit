# std-core-showcase

This showcase proves a sound, narrow slice of Rust `core` using the standard
library's own tests and no source changes.

Claimed slice:

- Source: the pinned Rust `1.96.0` toolchain's `rust-src` component, under
  `library/coretests`. The run script installs `rust-src` for that pinned
  toolchain and does not use CI's active default std source.
- Spec: selected scalar direct call-result `assert_eq!` rows from std/core own
  tests:
  - `coretests/tests/cmp.rs`: integer rows for
    `cmp::{min_by,max_by,min_by_key,max_by_key}`.
  - `coretests/tests/mem.rs`: generic type-arg-keyed rows for
    `size_of::<T>()` and `align_of::<T>()` from the non-cfg
    `size_of_basic` / `align_of_basic` vendor tests plus the active
    pinned-target pointer-width cfg tests.
  - `coretests/tests/time.rs`: finite decimal float rows for
    `Duration::div_duration_f{32,64}`.
  - `coretests/tests/fmt/mod.rs`: exact string rows for `to_string()`.
  - `coretests/tests/alloc.rs` and `coretests/tests/ops.rs`: pure
    method-chain predicate rows such as `layout.align_to(3).is_err()` and
    `(1u32..5).contains(&1u32)`.
  - `coretests/tests/atomic.rs`: stable-key compound RHS rows where the
    asserted value is a closed bitwise expression, such as
    `x.load(SeqCst) == 0xf731 & 0x137f`.
- Proof axis: `sugar mint` + `sugar verify` through `rust-test-assertions`
  emits `#euf#` call-result consistency rows and every claimed row discharges.
- Witness axis: the exact std vendor tests rerun with `cargo test --test
  coretests ... -- --exact`.

Named gaps toward full `coretests` coverage:

- Generic identity: type arguments are carried in the `#euf#` key for direct
  generic call results. The showcase claims the non-cfg `size_of_basic` and
  `align_of_basic` rows plus the active pointer-width cfg rows for the
  explicitly pinned target.
- Macros: bounded ASCII `assert_all!` / `assert_none!` expansion lives in the
  lifter, but this scalar std-core showcase does not claim broader macro
  surfaces.
- Float refinements: NaN, infinity, ordering, `-0.0`, and exponent-form
  literals stay out of this exact finite-value slice.
- Strings/chars: exact string equality is claimed here where it is a direct
  call-result value. Richer point-wise string predicates and ASCII char
  predicates are covered by the sibling `std-core-string-predicates`
  showcase; Unicode `char::is_alphabetic` remains a named residual.
- Method chains: pure immutable method-chain predicates with stable receiver
  identity are claimed. Stateful chains or tests that reassign a receiver name
  remain out of scope until the key can carry temporal identity.
- CFG-sensitive tests: only assertions whose `#[cfg]` predicates are active
  under the explicitly pinned Rust target are claimed. Inactive and ambiguous
  cfg predicates remain named residuals.
- Complex terms: closed bitwise-expression RHS terms are claimed where the
  call-result key is stable. Nested calls with non-value callees, stateful
  method chains, non-direct-call results, and unsupported expression forms stay
  out of the claimed slice.

The run script requires representative integer, generic type-arg-keyed, active
cfg pointer-width, finite-float, string, and pure method-chain predicate rows,
and rejects any non-discharged claimed `#euf#` row. It is intentionally not a
full-`std` claim.

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
which close operator-expression RHS complex terms. The current known backlog is
1,077 items for that target.

| Gap type | Count | Representative std test/assertion |
| --- | ---: | --- |
| Generics | 30 | Closed for direct generic call-result identity in this slice by carrying type args in the `#euf#` key; active cfg-sensitive pointer-width variants are tracked under CFG-sensitive. |
| Macros | 40 | Broad macro surfaces remain residual here; bounded ASCII `assert_all!` / `assert_none!` expansion is handled by the lifter but is outside this showcase's claimed scalar slice. |
| Floats | 18 | `tests/num/const_from.rs`: `assert_eq!(FROM_F64, 42f64)` remains outside the exact finite direct call-result slice. |
| Strings/chars | 183 | `tests/alloc.rs::layout_debug_shows_log2_of_alignment`: expected string literal for `Layout` debug output; not a direct call-result equality row. |
| CFG-sensitive | 61 | Residual after 4 closed: active `tests/mem.rs` `#[cfg(target_pointer_width = "64")]` rows for `size_of::<usize>()`, `size_of::<*const usize>()`, `align_of::<usize>()`, and `align_of::<*const usize>()` are claimed when the pinned target cfg facts say `target_pointer_width = "64"`; inactive widths and other cfg-sensitive tests remain residuals. |
| Complex terms | 444 | `tests/alloc.rs::layout_errors`: remaining method-chain and complex expression shapes not in the pure literal/immutable receiver/closed-compound-value slice. |
| Other | 331 | `tests/alloc.rs::layout_round_up_to_align_edge_cases`: no liftable scalar assertion under the current surface. |

### Complex-Term Decomposition

The 479 complex-term bucket breaks down as:

| Sub-shape | Count | Representative std test/assertion |
| --- | ---: | --- |
| Method-chain predicates | 90 | Closed 17 pure rows in this slice; remaining examples include reassigned receiver cases such as `tests/ops.rs::test_range_bounds`: `r.contains(&0)` after `r` is rebound. |
| References, derefs, casts, unsafe blocks | 81 | `tests/array.rs::array_from_mut`: `assert_eq!(&value, "Hello World!")`. |
| Method chains returning compared values | 68 | `tests/array.rs::iterator_nth`: `assert_eq!(IntoIterator::into_iter(v.clone()).nth(i).unwrap(), v[i])`. |
| Residual unsupported term shapes | 61 | `tests/any.rs::any_fixed_vec`: `TypeId::of::<[u8; 3]>()` compared with a dynamic `type_id()`. |
| Operator / expression RHS | 48 | This slice closes stable-key atomic bitwise RHS rows such as `tests/atomic.rs::uint_and`: `assert_eq!(x.load(SeqCst), 0xf731 & 0x137f)`; residual rows include stateful/repeated receiver and pointer arithmetic forms needing temporal identity. |
| Array, slice, and tuple literals | 48 | `tests/array.rs::array_from_ref`: `assert_eq!(&[*VALUE], ARR)`. |
| Boolean operators / non-equality predicates | 25 | `tests/array.rs::array_mixed_equality_integers`: `assert!(array3 != slice3b)`. |
| Nested calls / constructors | 20 | `tests/async_iter/mod.rs::into_async_iter`: `assert_eq!(..., Poll::Ready(Some(0)))`. |
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
