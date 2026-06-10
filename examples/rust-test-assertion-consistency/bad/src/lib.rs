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
    fn scalar_contradiction() {
        assert_same(make_value(), 6);
        assert_same(make_value(), 7);
    }
}
