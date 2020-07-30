//! Defines time related datatypes and functions

use core::{num::NonZeroU64, time::Duration};

/// An absolute point in time.
///
/// The absolute value of `Timestamp`s should be treated as opaque. It is not
/// necessarily related to any calendar time.
/// `Timestamp`s should only be compared if they are are sourced from the same
/// clock.
///
/// `Timestamp`s are similar to the `Instant` data-type in the Rust standard
/// library, but can be created even without an available standard library.
///
/// The size of `Timestamp` is guaranteed to be consistent across platforms.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Copy, Clone, Hash)]
pub struct Timestamp(NonZeroU64);

/// A prechecked 1us value
const ONE_MICROSECOND: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(1) };

impl Timestamp {
    /// Tries to calculate a `Timestamp` based on the current `Timestamp` and
    /// adding the provided `Duration`. If this `Timestamp` is representable
    /// within the range of `Timestamp` it is returned as `Some(timestamp)`.
    /// Otherwise `None` is returned.
    pub fn checked_add(self, duration: Duration) -> Option<Self> {
        self.as_duration_impl()
            .checked_add(duration)
            .map(Self::from_duration_impl)
    }

    /// Tries to calculate a `Timestamp` based on the current `Timestamp` and
    /// subtracting the provided `Duration`. If this `Timestamp` is representable
    /// within the range of `Timestamp` it is returned as `Some(timestamp)`.
    /// Otherwise `None` is returned.
    pub fn checked_sub(self, duration: Duration) -> Option<Self> {
        self.as_duration_impl()
            .checked_sub(duration)
            .map(Self::from_duration_impl)
    }

    /// Returns the `Duration` which elapsed since an earlier `Timestamp`.
    /// If `earlier` is more recent, the method returns a `Duration` of 0.
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        self.checked_sub(earlier.as_duration_impl())
            .map(Self::as_duration_impl)
            .unwrap_or_default()
    }

    /// Creates a `Timestamp` from a `Duration` since the time source's epoch.
    /// This will treat the duration as an absolute point in time.
    ///
    /// # Safety
    /// This should only be used by time sources
    pub unsafe fn from_duration(duration: Duration) -> Self {
        Self::from_duration_impl(duration)
    }

    /// Creates a `Timestamp` from a `Duration` since the time source's epoch.
    fn from_duration_impl(duration: Duration) -> Self {
        // 2^64 microseconds is ~580,000 years so casting from a u128 should be ok
        debug_assert!(duration.as_micros() <= core::u64::MAX.into());
        let micros = duration.as_micros() as u64;
        // if the value is 0 then round up to 1us after the epoch
        let micros = NonZeroU64::new(micros).unwrap_or(ONE_MICROSECOND);
        Self(micros)
    }

    /// Converts the `Timestamp` into the `Duration` since the time source's epoch.
    ///
    /// # Safety
    /// This should only be used by time sources
    pub unsafe fn as_duration(self) -> Duration {
        Self::as_duration_impl(self)
    }

    /// Returns the timestamp as a [`Duration`] since the
    /// clock epoch.
    const fn as_duration_impl(self) -> Duration {
        Duration::from_micros(self.0.get())
    }
}

impl core::ops::Add<Duration> for Timestamp {
    type Output = Timestamp;

    fn add(self, rhs: Duration) -> Self::Output {
        Timestamp::from_duration_impl(self.as_duration_impl() + rhs)
    }
}

impl core::ops::AddAssign<Duration> for Timestamp {
    fn add_assign(&mut self, other: Duration) {
        *self = *self + other;
    }
}

impl core::ops::Sub for Timestamp {
    type Output = Duration;

    fn sub(self, rhs: Timestamp) -> Self::Output {
        self.as_duration_impl() - rhs.as_duration_impl()
    }
}

impl core::ops::Sub<Duration> for Timestamp {
    type Output = Timestamp;

    fn sub(self, rhs: Duration) -> Self::Output {
        Timestamp::from_duration_impl(self.as_duration_impl() - rhs)
    }
}

impl core::ops::SubAssign<Duration> for Timestamp {
    fn sub_assign(&mut self, other: Duration) {
        *self = *self - other;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_from_and_to_duration() {
        let ts1 = Timestamp::from_duration_impl(Duration::from_millis(100));
        let ts2 = Timestamp::from_duration_impl(Duration::from_millis(220));

        // Subtract timestamps to gain a duration
        assert_eq!(Duration::from_millis(120), ts2 - ts1);

        // Add duration to timestamp
        let ts3 = ts2 + Duration::from_millis(11);
        assert_eq!(Duration::from_millis(231), unsafe {
            Timestamp::as_duration(ts3)
        });

        // Subtract a duration from a timestamp
        let ts4 = ts3 - Duration::from_millis(41);
        assert_eq!(Duration::from_millis(190), unsafe {
            Timestamp::as_duration(ts4)
        });
    }

    fn timestamp_math(initial: Timestamp) {
        // Add Duration
        let mut ts1 = initial + Duration::from_millis(500);
        assert_eq!(Duration::from_millis(500), ts1 - initial);
        // AddAssign Duration
        ts1 += Duration::from_millis(100);
        assert_eq!(Duration::from_millis(600), ts1 - initial);
        // SubAssign Duration
        ts1 -= Duration::from_millis(50);
        assert_eq!(Duration::from_millis(550), ts1 - initial);
        // Sub Duration
        let ts2 = ts1 - Duration::from_millis(110);
        assert_eq!(Duration::from_millis(440), ts2 - initial);
        // Checked Sub
        assert!(ts2
            .checked_sub(Duration::from_secs(std::u64::MAX))
            .is_none());
        assert_eq!(Some(initial), ts2.checked_sub(Duration::from_millis(440)));
        // Checked Add
        let max_duration =
            Duration::from_secs(std::u64::MAX) + (Duration::from_secs(1) - Duration::from_nanos(1));
        assert_eq!(None, ts2.checked_add(max_duration));
        assert!(ts2.checked_add(Duration::from_secs(24 * 60 * 60)).is_some());

        // Saturating Timestamp sub
        let higher = initial + Duration::from_millis(200);
        assert_eq!(
            Duration::from_millis(200),
            higher.saturating_duration_since(initial)
        );
        assert_eq!(
            Duration::from_millis(0),
            initial.saturating_duration_since(higher)
        );
    }

    #[test]
    fn timestamp_math_test() {
        // Start at a high inital timestamp to let the overflow check work
        let initial = Timestamp::from_duration_impl(Duration::from_micros(1));
        timestamp_math(initial);

        // Start at a high inital timestamp to let the overflow check work
        let initial = Timestamp::from_duration_impl(Duration::from_micros(1u64 << 63));
        timestamp_math(initial);
    }
}