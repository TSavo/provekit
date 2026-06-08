use tokio::sync::Mutex;

pub async fn producer() -> i64 {
    tokio::task::yield_now().await;
    6
}

pub fn consumer(x: i64) -> i64 {
    assert!(x == 6);
    6
}

pub async fn edge() -> i64 {
    let m = Mutex::new(producer().await);
    {
        let x = consumer(*m.lock().await);
        x
    }
}

#[cfg(test)]
mod tests {
    use super::edge;

    #[tokio::test]
    async fn mutex_protected_data_satisfies_critical_section_pre() {
        assert_eq!(edge().await, 6);
    }
}
