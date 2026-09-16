//! Converts engine RPM into a 5-LED shift-light bar state. Pure function of
//! (rpm, rpm_idle, rpm_max, config) -- no HID/hardware knowledge, so it's
//! trivially unit-testable and reusable if a second LED-bar-shaped output
//! ever shows up (e.g. driving the aux display's own LEDs).

#[derive(Debug, Clone, Copy)]
pub struct ShiftLightCurve {
    /// Fraction (0.0-1.0) of the idle->max RPM range at which the first LED
    /// turns on.
    pub shift_point_pct: f32,
    /// Whether to flash all LEDs once RPM is at/above the redline.
    pub blink_at_redline: bool,
}

impl Default for ShiftLightCurve {
    fn default() -> Self {
        Self {
            shift_point_pct: 0.85,
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
    /// `rpm_idle`/`rpm_max` come straight from the telemetry frame for the
    /// current car, so the curve adapts per-vehicle automatically.
    pub fn evaluate(&self, rpm: f32, rpm_idle: f32, rpm_max: f32) -> LedBarState {
        if rpm_max <= rpm_idle {
            return LedBarState::default();
        }

        let range = rpm_max - rpm_idle;
        let normalized = ((rpm - rpm_idle) / range).clamp(0.0, 1.2);

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

        // Map [shift_point_pct, 1.0) onto 5 LEDs, lighting from the outside
        // in as RPM climbs toward redline.
        let span = 1.0 - self.shift_point_pct;
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
            shift_point_pct: 0.8,
            blink_at_redline: true,
        }
    }

    #[test]
    fn below_shift_point_is_dark() {
        let s = curve().evaluate(3000.0, 1000.0, 7000.0); // ~33%
        assert_eq!(s.bits, 0);
        assert!(!s.blink);
    }

    #[test]
    fn at_redline_lights_all_and_blinks() {
        let s = curve().evaluate(7000.0, 1000.0, 7000.0);
        assert_eq!(s.bits, 0x1f);
        assert!(s.blink);
    }

    #[test]
    fn ramps_up_between_shift_point_and_redline() {
        let low = curve().evaluate(6100.0, 1000.0, 7000.0);
        let high = curve().evaluate(6800.0, 1000.0, 7000.0);
        assert!(low.bits.count_ones() <= high.bits.count_ones());
        assert!(!low.blink && !high.blink);
    }
}
