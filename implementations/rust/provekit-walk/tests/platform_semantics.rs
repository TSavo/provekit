// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

#[test]
fn rust_walk_exports_lift_side_platform_semantics_declaration() {
    let declaration = provekit_walk::platform_semantics::declaration();
    let dimensions = declaration
        .tags
        .iter()
        .flat_map(|tag| tag.dimensions.keys().map(String::as_str))
        .collect::<BTreeSet<_>>();

    assert_eq!(declaration.tags.len(), 21);
    assert_eq!(
        dimensions,
        BTreeSet::from([
            "ArithmeticOverflow",
            "BitwiseSemantics",
            "IntegerDivisionRounding",
            "NullSemantics",
            "ShiftMode",
        ])
    );
}
