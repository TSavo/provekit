pub async fn async_value() -> i32 {
    tokio::task::yield_now().await;
    6
}

#[cfg(test)]
mod tests {
    use super::async_value;

    #[tokio::test]
    async fn tokio_await_scalar_contradiction() {
        assert_eq!(async_value().await, 6);
        assert_eq!(async_value().await, 7);
    }
}
