/// C-callable wrapper over the real base64 crate's decoded_len_estimate.
///
/// This is the native side of the Panama FFM showcase. The Java consumer
/// calls this function via Linker.downcallHandle + LOOKUP.find("decoded_len_estimate").
///
/// The Rust vendor proof (minted from base64's own tests) asserts:
///   assert_eq!(3, decoded_len_estimate(4))
///
/// The bridge lifter maps the Java assertEquals(N, decoded_len_estimate(4)) callsite
/// to this contract. Good: N=3 (consistent). Bad: N=4 (contradicts → refuted).
#[no_mangle]
pub extern "C" fn decoded_len_estimate(encoded_len: u64) -> u64 {
    base64::decoded_len_estimate(encoded_len as usize) as u64
}
