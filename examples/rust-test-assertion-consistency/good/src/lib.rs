pub fn make_value() -> i32 {
    6
}

#[cfg(test)]
mod tests {
    use super::make_value;

    fn assert_same(actual: i32, expected: i32) {
        assert_eq!(actual, expected);
    }

    #[test]
    fn scalar_is_six() {
        assert_same(make_value(), 6);
    }
}
