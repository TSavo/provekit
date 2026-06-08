pub async fn producer() -> i64 {
    tokio::task::yield_now().await;
    6
}

pub fn consumer(x: i64) -> i64 {
    assert!(x == 6);
    6
}

pub async fn edge() -> i64 {
    consumer(producer().await)
}

#[cfg(test)]
mod tests {
    use super::edge;

    #[tokio::test]
    async fn awaited_post_satisfies_consumer_pre() {
        assert_eq!(edge().await, 6);
    }
}
