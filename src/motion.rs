use std::time::{Duration, Instant};

pub struct Motion {
    started: Option<Instant>,
    duration: Duration,
    pub reduced: bool,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            started: None,
            duration: Duration::from_millis(220),
            reduced: false,
        }
    }
}
impl Motion {
    pub fn start(&mut self) {
        self.started = if self.reduced {
            None
        } else {
            Some(Instant::now())
        };
    }
    pub fn sample(&mut self) -> (f32, bool) {
        self.at(Instant::now())
    }
    pub fn at(&mut self, now: Instant) -> (f32, bool) {
        let Some(start) = self.started else {
            return (1.0, false);
        };
        let t = now.saturating_duration_since(start).as_secs_f32() / self.duration.as_secs_f32();
        if t >= 1.0 {
            self.started = None;
            return (1.0, false);
        }
        let p = 1.0 - (1.0 - t).powi(3);
        (p, true)
    }
    pub fn active(&self) -> bool {
        self.started.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn animation_finishes_and_requires_no_idle_ticks() {
        let mut motion = Motion::default();
        motion.start();
        let started = motion.started.unwrap();
        assert!(motion.at(started + Duration::from_millis(100)).1);
        assert_eq!(motion.at(started + Duration::from_secs(1)), (1.0, false));
        assert!(!motion.active());
        motion.reduced = true;
        motion.start();
        assert_eq!(motion.sample(), (1.0, false));
    }
}
