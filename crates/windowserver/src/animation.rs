//! Animation subsystem.
//!
//! Driven by real elapsed wall-clock time (integer microseconds), not by how
//! often it happens to get polled. `present()` only runs in response to
//! input (see `server.rs`), so a step-per-poll animation would run at
//! whatever rate its caller happens to trigger redraws at: fast during a
//! burst of mouse movement, then frozen solid the instant the pointer stops
//! moving -- exactly the "only shrinks while I'm moving the mouse" bug this
//! was rewritten to fix. Timestamping instead of counting fixes both: total
//! duration is the same real time regardless of how many times `step` gets
//! called along the way, and a caller can query as often or as rarely as it
//! likes without changing that.
//!
//! This module itself never reads the clock -- no target this crate builds
//! for is guaranteed to have one wired up through its host ABI. The caller
//! (`windowserver-nxu`'s `ws_present`/`ws_pointer_move`/`ws_pointer_button`,
//! which *do* have a monotonic microsecond timer available) supplies `now_us`
//! as a plain argument.

/// A scalar that eases from `ONE` up to a bumped peak, holds briefly, then
/// eases back down -- all timed in real microseconds, in Q8.8-ish fixed
/// point (`ONE` == 1.0x). Integer-only: no target this crate builds for has
/// a hardware FPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EasedScale {
    current: u32,
    peak: u32,
    /// `None` at rest. Set by `bump()`; cleared once the shrink leg finishes.
    triggered_at_us: Option<u64>,
}

impl EasedScale {
    pub const ONE: u32 = 256;

    // Modeled on macOS's own "shake to locate" pacing: a quick pop up to
    // peak size, a short hold so it actually registers, then a deliberate
    // ease back down. All three are now real durations, not event counts.
    const GROW_DURATION_US: u64 = 120_000;
    const HOLD_DURATION_US: u64 = 250_000;
    const SHRINK_DURATION_US: u64 = 500_000;

    pub const fn new() -> Self {
        Self { current: Self::ONE, peak: Self::ONE, triggered_at_us: None }
    }

    pub const fn current(&self) -> u32 {
        self.current
    }

    /// Start easing up to `peak`, timestamped against the caller's clock.
    /// Re-bumping mid-animation (another shake before the last one settled)
    /// simply restarts the clock from `now_us`.
    pub fn bump(&mut self, peak: u32, now_us: u64) {
        self.peak = peak;
        self.triggered_at_us = Some(now_us);
    }

    /// Recompute `current` from elapsed wall-clock time and report whether
    /// it changed, so the caller knows to invalidate whatever this value
    /// sizes. Safe to call as often as convenient (e.g. once per presented
    /// frame) or as rarely as input arrives -- unlike a tick-counted
    /// animation, how often this is called does not change how long the
    /// animation takes, only how finely its progress gets sampled.
    pub fn step(&mut self, now_us: u64) -> bool {
        let Some(start) = self.triggered_at_us else { return false; };
        let elapsed = now_us.saturating_sub(start);

        let grow_end = Self::GROW_DURATION_US;
        let hold_end = grow_end + Self::HOLD_DURATION_US;
        let shrink_end = hold_end + Self::SHRINK_DURATION_US;

        let next = if elapsed >= shrink_end {
            self.triggered_at_us = None;
            Self::ONE
        } else if elapsed >= hold_end {
            Self::lerp(self.peak, Self::ONE, elapsed - hold_end, Self::SHRINK_DURATION_US)
        } else if elapsed >= grow_end {
            self.peak
        } else {
            Self::lerp(Self::ONE, self.peak, elapsed, grow_end)
        };

        if next == self.current {
            return false;
        }

        self.current = next;
        true
    }

    fn lerp(from: u32, to: u32, elapsed: u64, duration: u64) -> u32 {
        if duration == 0 {
            return to;
        }

        let delta = to as i64 - from as i64;
        let elapsed = elapsed.min(duration) as i64;
        (from as i64 + delta * elapsed / duration as i64) as u32
    }
}

impl Default for EasedScale {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_grows_then_eases_back_to_one() {
        let mut scale = EasedScale::new();
        assert_eq!(scale.current(), EasedScale::ONE);

        scale.bump(EasedScale::ONE * 3, 1_000_000);
        assert!(scale.step(1_000_000 + 10_000));
        assert!(scale.current() > EasedScale::ONE, "should have started growing");

        assert_eq!(scale.step(1_000_000 + EasedScale::GROW_DURATION_US), true);
        assert_eq!(scale.current(), EasedScale::ONE * 3, "should be at peak once grow duration elapses");

        // Still holding at peak partway through the hold window.
        scale.step(1_000_000 + EasedScale::GROW_DURATION_US + EasedScale::HOLD_DURATION_US / 2);
        assert_eq!(scale.current(), EasedScale::ONE * 3);

        // Long past every leg: settled back to rest.
        let far_future = 1_000_000
            + EasedScale::GROW_DURATION_US
            + EasedScale::HOLD_DURATION_US
            + EasedScale::SHRINK_DURATION_US
            + 1;
        scale.step(far_future);
        assert_eq!(scale.current(), EasedScale::ONE);
    }

    #[test]
    fn idle_scale_never_reports_a_change() {
        let mut scale = EasedScale::new();
        assert!(!scale.step(123_456));
        assert_eq!(scale.current(), EasedScale::ONE);
    }

    #[test]
    fn duration_is_independent_of_how_often_step_is_polled() {
        // Sparse polling (few, widely-spaced calls).
        let mut sparse = EasedScale::new();
        sparse.bump(EasedScale::ONE * 3, 0);
        sparse.step(EasedScale::GROW_DURATION_US + EasedScale::HOLD_DURATION_US + EasedScale::SHRINK_DURATION_US / 2);
        let sparse_mid_shrink = sparse.current();

        // Dense polling (many calls at small increments) covering the same
        // wall-clock span must land on the same value -- polling rate must
        // not change animation speed.
        let mut dense = EasedScale::new();
        dense.bump(EasedScale::ONE * 3, 0);
        let target_time = EasedScale::GROW_DURATION_US + EasedScale::HOLD_DURATION_US + EasedScale::SHRINK_DURATION_US / 2;
        let mut t = 0u64;
        while t < target_time {
            t += 1000;
            dense.step(t.min(target_time));
        }
        dense.step(target_time);

        assert_eq!(sparse_mid_shrink, dense.current());
    }

    #[test]
    fn settles_in_one_shot_even_if_never_polled_in_between() {
        // The old bug: an animation that only advances when re-triggered by
        // input would stay frozen at peak forever if nothing ever calls
        // step() again while the pointer sits still. Here, after reaching
        // peak, we skip straight to a single step() call long past every
        // leg -- with no polling anywhere in between -- and confirm it still
        // settles correctly, proving progress depends on elapsed time, not
        // on a count of prior calls.
        let mut scale = EasedScale::new();
        scale.bump(EasedScale::ONE * 3, 0);
        scale.step(EasedScale::GROW_DURATION_US);
        assert_eq!(scale.current(), EasedScale::ONE * 3, "should be at peak");

        let far_future = EasedScale::GROW_DURATION_US + EasedScale::HOLD_DURATION_US + EasedScale::SHRINK_DURATION_US + 1;
        assert!(scale.step(far_future));
        assert_eq!(scale.current(), EasedScale::ONE);
    }
}
