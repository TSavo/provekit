pub fn make_value() -> i32 {
    6
}

#[cfg(test)]
mod tests {
    use super::make_value;

    #[test]
    fn scalar_is_six() {
        assert_eq!(make_value(), 6);
    }
}
