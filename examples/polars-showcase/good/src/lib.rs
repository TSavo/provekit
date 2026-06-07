use polars::prelude::*;

pub fn scalar_sum() -> i32 {
    let values = Series::new("values".into(), [1i32, 2, 3]);
    values.i32().expect("i32 series").sum().expect("sum")
}

#[cfg(test)]
mod tests {
    use super::scalar_sum;

    #[test]
    fn polars_scalar_sum_is_six() {
        assert_eq!(scalar_sum(), 6);
    }
}
