use tokio::sync::mpsc;

pub async fn producer() -> i64 {
    tokio::task::yield_now().await;
    5
}

pub fn consumer(x: i64) -> i64 {
    assert!(x == 6);
    6
}

pub async fn edge() -> i64 {
    let (tx, mut rx) = mpsc::channel::<i64>(1);
    tx.send(producer().await)
        .await
        .expect("local receiver remains alive");
    consumer(rx.recv().await.expect("one value was sent"))
}

#[cfg(test)]
mod tests {
    use super::edge;

    #[tokio::test]
    async fn channel_send_post_does_not_satisfy_recv_consumer_pre() {
        assert_eq!(edge().await, 6);
    }
}
