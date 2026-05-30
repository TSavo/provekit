pub fn double(x: i64) -> i64 {
    x * 2
}

#[cfg(test)]
mod native_tests {
    use super::double;

    #[test]
    fn double_three_is_six() {
        assert_eq!(double(3), 6);
    }
}

#[cfg(test)]
include!("provekit_emitted.rs");
