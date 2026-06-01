// SPDX-License-Identifier: Apache-2.0

use libprovekit::concept::panic_freedom;

#[test]
fn panic_freedom_constants_keep_existing_wire_tokens() {
    assert_eq!(panic_freedom::IS_OK, "is_ok");
    assert_eq!(panic_freedom::IS_ERR, "is_err");
    assert_eq!(panic_freedom::IS_SOME, "is_some");
    assert_eq!(panic_freedom::IS_NONE, "is_none");
    assert_eq!(panic_freedom::CF_GUARDED, "cf_guarded");
    assert_eq!(panic_freedom::CF_ITE, "cf_ite");
    assert_eq!(panic_freedom::METHOD_UNWRAP, "method:unwrap");
    assert_eq!(panic_freedom::METHOD_EXPECT, "method:expect");
    assert_eq!(panic_freedom::METHOD_UNWRAP_ERR, "method:unwrap_err");
}
