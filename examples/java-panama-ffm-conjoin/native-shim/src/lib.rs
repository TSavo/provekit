#[no_mangle]
pub extern "C" fn decoded_len_estimate(encoded_len: u64) -> u64 {
    base64::decoded_len_estimate(encoded_len as usize) as u64
}
