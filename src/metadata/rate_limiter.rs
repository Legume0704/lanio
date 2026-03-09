use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{interval, Duration};

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
}

impl RateLimiter {
    pub fn new(max_requests: usize) -> Self {
        let semaphore = Arc::new(Semaphore::new(max_requests));

        // Background task to refill permits every 10 seconds
        let sem_clone = semaphore.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            loop {
                interval.tick().await;
                // Add back permits up to max
                let available = sem_clone.available_permits();
                if available < max_requests {
                    sem_clone.add_permits(max_requests - available);
                }
            }
        });

        Self { semaphore }
    }

    pub async fn acquire(&self) {
        let _ = self.semaphore.acquire().await.expect("Semaphore closed");
    }
}
