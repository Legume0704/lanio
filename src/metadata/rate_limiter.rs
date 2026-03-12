use std::sync::Arc;
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::{interval_at, Duration, Instant};

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    _task: tokio::task::JoinHandle<()>,
}

impl RateLimiter {
    pub fn new(max_requests: usize) -> Self {
        let semaphore = Arc::new(Semaphore::new(max_requests));

        // Background task to refill permits every 10 seconds.
        // interval_at delays the first tick so the window doesn't reset immediately on startup.
        let sem_clone = semaphore.clone();
        let task = tokio::spawn(async move {
            let window = Duration::from_secs(10);
            let mut interval = interval_at(Instant::now() + window, window);
            loop {
                interval.tick().await;
                // Add back permits up to max
                let available = sem_clone.available_permits();
                if available < max_requests {
                    sem_clone.add_permits(max_requests - available);
                }
            }
        });

        Self {
            semaphore,
            _task: task,
        }
    }

    pub async fn acquire(&self) -> SemaphorePermit<'_> {
        self.semaphore.acquire().await.expect("Semaphore closed")
    }
}

impl Drop for RateLimiter {
    fn drop(&mut self) {
        self._task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_permit_held_until_dropped() {
        let limiter = RateLimiter::new(1);
        let permit = limiter.acquire().await;

        // Permit is still held — second acquire should block
        let blocked = tokio::time::timeout(Duration::from_millis(50), limiter.acquire()).await;
        assert!(blocked.is_err(), "second acquire should block while first permit is held");

        drop(permit);

        // After release, next acquire should succeed immediately
        let succeeded = tokio::time::timeout(Duration::from_millis(50), limiter.acquire()).await;
        assert!(succeeded.is_ok(), "acquire should succeed after permit is released");
    }

    #[tokio::test]
    async fn test_multiple_permits_exhausted() {
        let limiter = RateLimiter::new(3);
        let p1 = limiter.acquire().await;
        let p2 = limiter.acquire().await;
        let p3 = limiter.acquire().await;

        // All 3 permits taken — 4th should block
        let blocked = tokio::time::timeout(Duration::from_millis(50), limiter.acquire()).await;
        assert!(blocked.is_err(), "4th acquire should block when all permits are held");

        drop(p1);
        drop(p2);
        drop(p3);
    }

    #[tokio::test]
    async fn test_drop_aborts_background_task() {
        // Verify dropping RateLimiter does not panic
        let limiter = RateLimiter::new(10);
        drop(limiter);
    }
}
