use tokio::time::{Duration, Instant};

#[derive(Debug)]
pub struct WindowedCounter {
    buckets: [i64; NUM_BUCKETS],
    bucket_ms: u128,
    epoch: Instant,
    bucket: usize,
}

const NUM_BUCKETS: usize = 8;

impl WindowedCounter {
    pub fn new(window: Duration) -> Self {
        WindowedCounter {
            buckets: [0; NUM_BUCKETS],
            bucket_ms: (window / NUM_BUCKETS as u32).as_millis(),
            epoch: Instant::now(),
            bucket: 0,
        }
    }
    pub fn add(&mut self, amount: i64) {
        self.advance();
        *self.curr_bucket() += amount;
    }

    pub fn total(&mut self) -> i64 {
        self.advance();
        self.buckets.iter().sum()
    }

    pub fn clear(&mut self) {
        self.buckets.fill(0);
        self.epoch = Instant::now();
    }

    #[inline]
    fn curr_bucket(&mut self) -> &mut i64 {
        &mut self.buckets[self.bucket]
    }

    fn advance(&mut self) {
        let now = Instant::now();
        let elapsed_ms = now.duration_since(self.epoch).as_millis();

        // we are still within the same bucket, do nothing.
        if elapsed_ms < self.bucket_ms {
            return;
        }

        self.bucket = (self.bucket + 1) % NUM_BUCKETS;
        let skipped = (((elapsed_ms / self.bucket_ms) - 1) as usize).min(NUM_BUCKETS);

        // we advanced past more than one bucket, zero all the skipped buckets.
        if skipped > 0 {
            let skipped_right = skipped.min(NUM_BUCKETS - self.bucket);
            self.buckets[self.bucket..self.bucket + skipped_right].fill(0);
            let skipped_left = skipped - skipped_right;
            self.buckets[..skipped_left].fill(0);
            self.bucket = (self.bucket + skipped) % NUM_BUCKETS;
        }

        *self.curr_bucket() = 0;
        self.epoch = now;
    }
}
