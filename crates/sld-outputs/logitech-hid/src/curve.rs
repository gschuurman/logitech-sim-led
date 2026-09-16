//! Converts engine RPM into a 5-LED shift-light bar state. Pure function of
//! (rpm, rpm_idle, rpm_max, config) -- no HID/hardware knowledge, so it's
//! trivially unit-testable and reusable if a second LED-bar-shaped output
//! ever shows up (e.g. driving the aux display's own LEDs).

#[derive(Debug, Clone, Copy)]
pub struct ShiftLightCurve {
    /// Fraction (0.0-1.0) of `rpm_max` (redline) at which the first LED
    /// turns on.
    pub shift_point_pct: f32,
    /// Fraction (0.0-1.0) of `rpm_max` at which all 5 LEDs are lit. From
    /// here up to redline the bar stays solidly full -- this is the "top
    /// N% should already be maxed out" zone, not more ramping.
    pub full_bar_pct: f32,
    /// Whether to flash all LEDs once RPM is at/above the redline.
    pub blink_at_redline: bool,
}

impl Default for ShiftLightCurve {
    fn default() -> Self {
        Self {
            // First LED at 60% of redline, full bar by 80% -- leaves the
            // top 20% of the rev range solidly lit rather than still
            // ramping right up to redline.
            shift_point_pct: 0.6,
            full_bar_pct: 0.8,
            blink_at_redline: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LedBarState {
    pub bits: u8,
    pub blink: bool,
}

impl ShiftLightCurve {
    /// `rpm_max` (redline) comes straight from the telemetry frame for the
    /// current car, so the curve adapts per-vehicle automatically. Deliberately
    /// *not* relative to idle RPM: percentage-of-idle-to-max-range was the
    /// bug here previously (a nonzero idle RPM pushed the whole ramp later
    /// than `shift_point_pct` implied -- confirmed against real telemetry:
    /// only 2 of 5 LEDs lit at 7335/8000, 91.7% of redline, with the old
    /// idle-relative math and the old defaults). A shift light means
    /// "percent of redline", full stop.
    pub fn evaluate(&self, rpm: f32, rpm_max: f32) -> LedBarState {
        if rpm_max <= 0.0 {
            return LedBarState::default();
        }

        let normalized = (rpm / rpm_max).clamp(0.0, 1.2);

        if normalized >= 1.0 {
            return LedBarState {
                bits: 0x1f,
                blink: self.blink_at_redline,
            };
        }

        if normalized < self.shift_point_pct {
            return LedBarState {
                bits: 0x00,
                blink: false,
            };
        }

        if normalized >= self.full_bar_pct {
            return LedBarState {
                bits: 0x1f,
                blink: false,
            };
        }

        // Map [shift_point_pct, full_bar_pct) onto 5 LEDs, lighting from
        // the outside in as RPM climbs.
        let span = self.full_bar_pct - self.shift_point_pct;
        let progress = ((normalized - self.shift_point_pct) / span).clamp(0.0, 0.999);
        let lit = 1 + (progress * 5.0) as u8; // 1..=5 LEDs lit
        let bits = (0..lit).fold(0u8, |acc, i| acc | (1 << i));

        LedBarState { bits, blink: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve() -> ShiftLightCurve {
        ShiftLightCurve {
            shift_point_pct: 0.6,
            full_bar_pct: 0.8,
            blink_at_redline: true,
        }
    }

    #[test]
    fn below_shift_point_is_dark() {
        let s = curve().evaluate(3000.0, 8000.0); // ~37.5%
        assert_eq!(s.bits, 0);
        assert!(!s.blink);
    }

    #[test]
    fn at_redline_lights_all_and_blinks() {
        let s = curve().evaluate(8000.0, 8000.0);
        assert_eq!(s.bits, 0x1f);
        assert!(s.blink);
    }

    #[test]
    fn ramps_up_between_shift_point_and_full_bar() {
        let low = curve().evaluate(5000.0, 8000.0); // 62.5%
        let high = curve().evaluate(7000.0, 8000.0); // 87.5%
        assert!(low.bits.count_ones() <= high.bits.count_ones());
        assert!(!low.blink && !high.blink);
    }

    #[test]
    fn top_of_range_is_solidly_full_before_redline() {
        // Regression test for the original bug report: with the idle-
        // relative math this used to compute, 7335/8000 (91.7% of
        // redline) only lit 2 of 5 LEDs. It should be fully lit anywhere
        // from full_bar_pct up to (not including) redline.
        let s = curve().evaluate(7335.0, 8000.0);
        assert_eq!(s.bits, 0x1f);
        assert!(!s.blink, "full bar below redline should not blink yet");
    }
}
