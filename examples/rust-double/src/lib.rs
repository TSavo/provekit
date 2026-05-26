pub fn double(x: i64) -> i64 {
    x * 2
}

#[cfg(test)]
mod tests {
    use super::double;

    #[test]
    fn double_three_is_six() {
        assert_eq!(double(3), 6);
    }
}
