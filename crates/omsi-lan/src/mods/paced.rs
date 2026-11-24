use std::io::{ErrorKind, Read};
use std::time::{Duration, Instant};

const GREETING: Duration = Duration::from_secs(660);
const WINDOW: Duration = Duration::from_secs(60);
const MIN_BYTES: u64 = 1024 * 1024;
const WHOLE: Duration = Duration::from_secs(6 * 3600);

pub(super) struct Paced<R> {
    inner: R,
    until: Option<Instant>,
    ends: Instant,
    window: Duration,
    min_bytes: u64,
    busy: Duration,
    bytes: u64,
}

impl<R> Paced<R> {
    pub fn greeting(inner: R) -> Paced<R> {
        Paced {
            inner,
            until: Some(Instant::now() + GREETING),
            ends: Instant::now() + WHOLE,
            window: WINDOW,
            min_bytes: MIN_BYTES,
            busy: Duration::ZERO,
            bytes: 0,
        }
    }

    pub fn pace(&mut self) {
        self.until = None;
        self.busy = Duration::ZERO;
        self.bytes = 0;
    }

    pub fn get_ref(&self) -> &R {
        &self.inner
    }
}

impl<R: Read> Read for Paced<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let t0 = Instant::now();
        if self.until.is_some_and(|u| t0 > u) {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "the host did not greet in time",
            ));
        }
        if t0 > self.ends {
            return Err(std::io::Error::new(
                ErrorKind::TimedOut,
                "the host's mods took too long",
            ));
        }
        let n = self.inner.read(buf)?;
        if self.until.is_none() {
            self.busy += t0.elapsed();
            self.bytes += n as u64;
            if self.busy >= self.window {
                if self.bytes < self.min_bytes {
                    return Err(std::io::Error::new(
                        ErrorKind::TimedOut,
                        "the host sends too slowly",
                    ));
                }
                self.busy = Duration::ZERO;
                self.bytes = 0;
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Trickle;

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            std::thread::sleep(Duration::from_millis(5));
            buf[0] = b'x';
            Ok(1)
        }
    }

    #[test]
    fn a_host_that_trickles_is_given_up() {
        let mut p = Paced::greeting(Trickle);
        p.pace();
        p.window = Duration::from_millis(50);
        p.min_bytes = 1000;
        let mut b = [0u8; 16];
        let err = (0..1000)
            .find_map(|_| p.read(&mut b).err())
            .expect("given up");
        assert_eq!(err.kind(), ErrorKind::TimedOut);
    }

    #[test]
    fn a_greeting_that_never_ends_is_given_up() {
        let mut p = Paced::greeting(Trickle);
        p.until = Some(Instant::now() + Duration::from_millis(30));
        let mut b = [0u8; 16];
        let err = (0..1000)
            .find_map(|_| p.read(&mut b).err())
            .expect("given up");
        assert_eq!(err.kind(), ErrorKind::TimedOut);
    }

    #[test]
    fn a_download_that_never_ends_is_given_up() {
        let mut p = Paced::greeting(std::io::repeat(b'x'));
        p.pace();
        p.ends = Instant::now() + Duration::from_millis(30);
        let mut b = [0u8; 16];
        let err = (0..100_000).find_map(|i| {
            if i % 100 == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
            p.read(&mut b).err()
        });
        assert_eq!(err.expect("given up").kind(), ErrorKind::TimedOut);
    }

    #[test]
    fn a_steady_host_goes_on() {
        let mut p = Paced::greeting(std::io::repeat(b'x'));
        p.pace();
        p.window = Duration::from_millis(1);
        p.min_bytes = 1;
        let mut b = [0u8; 4096];
        for _ in 0..1000 {
            assert!(p.read(&mut b).unwrap() > 0);
        }
    }
}
