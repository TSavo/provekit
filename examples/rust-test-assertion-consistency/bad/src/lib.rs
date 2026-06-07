pub fn make_value() -> i32 {
    6
}

#[cfg(test)]
mod tests {
    use super::make_value;

    #[test]
    fn scalar_contradiction() {
        assert_eq!(make_value(), 6);
        assert_eq!(make_value(), 7);
    }
}
