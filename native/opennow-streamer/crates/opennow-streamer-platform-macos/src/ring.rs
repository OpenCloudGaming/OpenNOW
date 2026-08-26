use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct PcmRing {
    samples: Box<[UnsafeCell<f32>]>,
    read: AtomicUsize,
    write: AtomicUsize,
}

// Exactly one Opus worker calls push and exactly one CoreAudio callback calls pop_into. Acquire and
// release publication prevents either side from accessing a slot while the other side owns it.
unsafe impl Send for PcmRing {}
unsafe impl Sync for PcmRing {}

impl PcmRing {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0 && capacity < usize::MAX / 2);
        Self {
            samples: (0..capacity).map(|_| UnsafeCell::new(0.0)).collect(),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
        }
    }

    pub(crate) fn push(&self, input: &[f32]) -> usize {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let available = self.samples.len().saturating_sub(write.wrapping_sub(read));
        let count = input.len().min(available);
        for (offset, sample) in input[..count].iter().enumerate() {
            let index = write.wrapping_add(offset) % self.samples.len();
            unsafe { *self.samples[index].get() = *sample };
        }
        self.write
            .store(write.wrapping_add(count), Ordering::Release);
        count
    }

    pub(crate) fn pop_into(&self, output: &mut [f32]) -> usize {
        let read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        let count = output.len().min(write.wrapping_sub(read));
        for (offset, sample) in output[..count].iter_mut().enumerate() {
            let index = read.wrapping_add(offset) % self.samples.len();
            unsafe { *sample = *self.samples[index].get() };
        }
        self.read.store(read.wrapping_add(count), Ordering::Release);
        count
    }

    pub(crate) fn clear(&self) {
        let write = self.write.load(Ordering::Acquire);
        self.read.store(write, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn wraps_and_preserves_pcm_order() {
        let ring = PcmRing::new(4);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0]), 3);
        let mut first = [0.0; 2];
        assert_eq!(ring.pop_into(&mut first), 2);
        assert_eq!(first, [1.0, 2.0]);
        assert_eq!(ring.push(&[4.0, 5.0, 6.0]), 3);
        let mut second = [0.0; 4];
        assert_eq!(ring.pop_into(&mut second), 4);
        assert_eq!(second, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn producer_and_consumer_can_run_concurrently() {
        const COUNT: usize = 20_000;
        let ring = Arc::new(PcmRing::new(256));
        let producer = {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                for value in 0..COUNT {
                    let sample = value as f32;
                    while ring.push(&[sample]) == 0 {
                        thread::yield_now();
                    }
                }
            })
        };
        let mut expected = 0usize;
        let mut sample = [0.0];
        while expected < COUNT {
            if ring.pop_into(&mut sample) == 0 {
                thread::yield_now();
                continue;
            }
            assert_eq!(sample[0], expected as f32);
            expected += 1;
        }
        producer.join().unwrap();
    }
}
