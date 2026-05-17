// SPDX-License-Identifier: Apache-2.0

use libprovekit::core::types::PlatformSemanticsDeclaration;

pub fn declaration() -> PlatformSemanticsDeclaration {
    libprovekit::core::platform_semantics_for_lower_target("rust")
        .expect("rust platform semantics declaration is wired through libprovekit")
}
