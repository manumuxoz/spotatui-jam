use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Token bucket with a fractional refill. One token is required per action.
#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

struct Entry {
    bucket: TokenBucket,
    last_seen: Instant,
}

/// One bucket per client IP. Old entries are pruned opportunistically so a
/// long-lived relay does not retain one entry per address forever.
pub struct KeyedLimiter {
    entries: Mutex<HashMap<IpAddr, Entry>>,
    capacity: f64,
    refill_per_sec: f64,
}

impl KeyedLimiter {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            capacity,
            refill_per_sec,
        }
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut entries = lock(&self.entries);
        if entries.len() > 4096 {
            let cutoff = now.checked_sub(Duration::from_secs(600)).unwrap_or(now);
            entries.retain(|_, entry| entry.last_seen > cutoff);
        }
        let entry = entries.entry(ip).or_insert_with(|| Entry {
            bucket: TokenBucket::new(self.capacity, self.refill_per_sec),
            last_seen: now,
        });
        entry.last_seen = now;
        entry.bucket.try_acquire()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::Ipv4Addr, thread, time::Duration};

    use super::{KeyedLimiter, TokenBucket};

    #[test]
    fn burst_is_limited() {
        let mut bucket = TokenBucket::new(10.0, 2.0);
        for _ in 0..10 {
            assert!(bucket.try_acquire());
        }
        assert!(!bucket.try_acquire());
    }

    #[test]
    fn tokens_refill_over_time() {
        let mut bucket = TokenBucket::new(2.0, 2.0);
        assert!(bucket.try_acquire());
        assert!(bucket.try_acquire());
        assert!(!bucket.try_acquire());

        thread::sleep(Duration::from_millis(550));
        assert!(bucket.try_acquire());
    }

    #[test]
    fn limiter_is_per_ip() {
        let limiter = KeyedLimiter::new(1.0, 0.0);
        let first = Ipv4Addr::new(192, 0, 2, 1).into();
        let second = Ipv4Addr::new(192, 0, 2, 2).into();
        assert!(limiter.allow(first));
        assert!(!limiter.allow(first));
        assert!(limiter.allow(second));
    }
}
