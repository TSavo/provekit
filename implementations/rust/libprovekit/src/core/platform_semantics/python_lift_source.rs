use crate::core::types::PlatformSemanticsDeclaration;
use provekit_ir_types::DimensionValueMemento;

use super::python_common;

pub fn declaration() -> PlatformSemanticsDeclaration {
    python_common::declaration()
}

pub fn dimension_values() -> Vec<DimensionValueMemento> {
    python_common::dimension_values()
}
