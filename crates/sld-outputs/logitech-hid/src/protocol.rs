//! HID output-report encoding for Logitech wheel RPM shift-light LEDs.
//!
//! Verified against the actively-maintained Linux `hid-lg4ff` driver
//! (<https://github.com/berarma/new-lg4ff>, files `hid-lg4ff.c` /
//! `hid-ids.h`), which is a stronger source than the earlier
//! "community reverse-engineering" framing this module used -- it's a
//! driver actively used to run force feedback on this exact hardware, not
//! a one-off packet capture. `hidapi` (used by `device.rs`) sends output
//! reports with the same report-id-as-first-byte framing on both Windows
//! and Linux, so one implementation here covers both platforms.
//!
//! ## The command itself
//!
//! `lg4ff_set_leds()` in that driver sends report id `0xf8`, sub-command
//! `0x12`, then a single byte bitmask of which of the 5 LEDs are lit (bit 0
//! = leftmost) -- see [`ClassicLedProtocol`]. It's registered for G27, G29,
//! and G923 **once the G923 is in its native/PC mode** (product id
//! `0xc266`): `hid-lg4ff.c` explicitly gates LED support on
//! `product_id == G27 || product_id == G29 || product_id == G923_WHEEL`.
//!
//! ## G923 PlayStation-mode needs a mode switch first
//!
//! A G923 connected in "PlayStation" mode enumerates as product id
//! `0xc267`, which the driver does *not* register LEDs for directly.
//! Instead, on connecting such a wheel, the driver sends a one-shot
//! "switch to native mode, with detach" command -- after which the wheel
//! disconnects and re-enumerates as `0xc266` (native mode), where the
//! normal LED command then applies. That switch command is notable for
//! using a *different* report id (`0x30`, not `0xf8`) -- see
//! [`known_mode_switches`] and `lg4ff_switch_from_ps_mode()` /
//! `lg4ff_mode_switch_30_g923` in the driver source.
//!
//! ## What's still inferred, not driver-confirmed
//!
//! - **G923 Xbox-mode** (`0xc26e`, `USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL`
//!   in the driver's `hid-ids.h`) is defined but never referenced anywhere
//!   else in `hid-lg4ff.c` -- no LED registration, no mode-switch handling.
//!   This module treats it as already-native (added to
//!   [`ClassicLedProtocol`]'s matches) on the reasoning that Xbox
//!   controllers don't need PS4-style mode negotiation on a PC the way the
//!   PlayStation variant does -- but that's an inference, not something
//!   the driver source confirms one way or the other.
//! - **G920** (`0xc262`) is explicitly *excluded* from `has_leds` in the
//!   driver (only G27/G29/G923-native get it). This doesn't necessarily
//!   mean the hardware lacks shift-light LEDs -- it may just mean the
//!   driver hasn't implemented it -- but there's no evidence here for what
//!   command would work, so it stays out of [`all_known_protocols`]; see
//!   [`known_unsupported_wheels`].
//! - **Driving Force GT** (`0xc29a`) is similarly excluded from
//!   `has_leds` in this driver, despite this module previously grouping it
//!   with G27/G29. Moved to the unsupported list for the same reason.

pub trait WheelLedProtocol: Send + Sync {
    /// Human readable id, e.g. "classic".
    fn id(&self) -> &'static str;

    /// USB vendor/product IDs this protocol applies to.
    fn matches(&self, vendor_id: u16, product_id: u16) -> bool;

    /// HID usage page this protocol's report must be written to. Wheels
    /// like the G923 enumerate as *several* HID interfaces under the same
    /// vendor/product id (a vendor-specific config interface, extra
    /// buttons, etc.) -- only the actual joystick/FFB collection accepts
    /// output reports in this shape; writing to any other interface opens
    /// fine but fails the `write()` (Windows: `ERROR_INVALID_PARAMETER`).
    /// Defaults to Generic Desktop (`0x01`), which is where that
    /// collection lives for every wheel this crate currently supports.
    fn usage_page(&self) -> u16 {
        0x0001
    }

    /// HID usage within [`Self::usage_page`]; `0x04` is "Joystick".
    fn usage(&self) -> u16 {
        0x0004
    }

    /// Build the raw HID report bytes for a given 5-bit LED mask (bit 0 =
    /// leftmost LED).
    fn encode_leds(&self, led_bits: u8) -> Vec<u8>;
}

pub const LOGITECH_VENDOR_ID: u16 = 0x046d;

pub const G27_PRODUCT_ID: u16 = 0xc29b;
pub const G29_PRODUCT_ID: u16 = 0xc24f;
pub const G920_PRODUCT_ID: u16 = 0xc262;
/// G923 in native/PC mode -- what the LED command actually targets.
pub const G923_PRODUCT_ID: u16 = 0xc266;
/// G923 in PlayStation-compatible mode -- needs [`known_mode_switches`]
/// run first; LEDs don't work directly against this product id.
pub const G923_PS_PRODUCT_ID: u16 = 0xc267;
/// G923 in Xbox-compatible mode -- treated as already-native; see module
/// docs for why this is an inference rather than a driver-confirmed fact.
pub const G923_XBOX_PRODUCT_ID: u16 = 0xc26e;
pub const DFGT_PRODUCT_ID: u16 = 0xc29a;

/// The `0xf8 0x12 <bits>` command: G27, G29, and G923 once in native mode
/// (directly, or after [`known_mode_switches`] has switched it there).
pub struct ClassicLedProtocol;

impl WheelLedProtocol for ClassicLedProtocol {
    fn id(&self) -> &'static str {
        "classic"
    }

    fn matches(&self, vendor_id: u16, product_id: u16) -> bool {
        vendor_id == LOGITECH_VENDOR_ID
            && matches!(
                product_id,
                G27_PRODUCT_ID | G29_PRODUCT_ID | G923_PRODUCT_ID | G923_XBOX_PRODUCT_ID
            )
    }

    fn encode_leds(&self, led_bits: u8) -> Vec<u8> {
        vec![0xf8, 0x12, led_bits & 0x1f, 0x00, 0x00, 0x00, 0x00]
    }
}

pub fn all_known_protocols() -> Vec<Box<dyn WheelLedProtocol>> {
    vec![Box::new(ClassicLedProtocol)]
}

/// A one-shot command that switches a wheel out of a compatibility mode
/// and into one [`ClassicLedProtocol`] (or a future protocol) recognizes.
/// The wheel is expected to disconnect and re-enumerate under
/// `expected_product_id_after_switch` after receiving this.
pub struct ModeSwitch {
    pub id: &'static str,
    pub matches_product_id: u16,
    /// HID usage page/usage of the interface to send `switch_report` to --
    /// same reasoning as [`WheelLedProtocol::usage_page`]: these wheels
    /// enumerate multiple HID interfaces under one product id, and only
    /// the joystick/FFB collection accepts this write.
    pub usage_page: u16,
    pub usage: u16,
    /// Full bytes to hand to `HidDevice::write` -- includes the leading
    /// report-id byte, which for this switch is `0x30`, not the usual
    /// `0xf8` (see module docs / `lg4ff_switch_from_ps_mode`).
    pub switch_report: [u8; 8],
    pub expected_product_id_after_switch: u16,
}

/// Known mode-switch handshakes. Currently just the G923 PlayStation-mode
/// one, which is the only one `hid-lg4ff.c` documents in enough detail to
/// reproduce with confidence.
pub fn known_mode_switches() -> Vec<ModeSwitch> {
    vec![ModeSwitch {
        id: "g923_ps_to_native",
        matches_product_id: G923_PS_PRODUCT_ID,
        usage_page: 0x0001,
        usage: 0x0004,
        switch_report: [0x30, 0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00],
        expected_product_id_after_switch: G923_PRODUCT_ID,
    }]
}

/// Logitech wheel product ids recognized by USB id but with no confirmed
/// LED command -- used only to print a clearer diagnostic ("found your
/// wheel, but don't know how to drive its LEDs yet") instead of a generic
/// "no wheel found".
pub fn known_unsupported_wheels() -> Vec<(u16, &'static str)> {
    vec![
        (G920_PRODUCT_ID, "G920"),
        (DFGT_PRODUCT_ID, "Driving Force GT"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_protocol_matches_g27_g29_and_g923_native_and_xbox() {
        let p = ClassicLedProtocol;
        assert!(p.matches(LOGITECH_VENDOR_ID, G27_PRODUCT_ID));
        assert!(p.matches(LOGITECH_VENDOR_ID, G29_PRODUCT_ID));
        assert!(p.matches(LOGITECH_VENDOR_ID, G923_PRODUCT_ID));
        assert!(p.matches(LOGITECH_VENDOR_ID, G923_XBOX_PRODUCT_ID));
        assert!(!p.matches(LOGITECH_VENDOR_ID, G923_PS_PRODUCT_ID));
        assert!(!p.matches(LOGITECH_VENDOR_ID, G920_PRODUCT_ID));
        assert!(!p.matches(LOGITECH_VENDOR_ID, DFGT_PRODUCT_ID));
        assert!(!p.matches(0x1234, G29_PRODUCT_ID));
    }

    #[test]
    fn classic_protocol_encodes_leds_as_f8_12_bits() {
        let p = ClassicLedProtocol;
        assert_eq!(
            p.encode_leds(0x1f),
            vec![0xf8, 0x12, 0x1f, 0x00, 0x00, 0x00, 0x00]
        );
        // High bits above the 5-LED mask are masked off.
        assert_eq!(
            p.encode_leds(0xff),
            vec![0xf8, 0x12, 0x1f, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn g923_ps_mode_switch_targets_native_product_id_with_report_id_0x30() {
        let switches = known_mode_switches();
        let g923_ps = switches
            .iter()
            .find(|s| s.matches_product_id == G923_PS_PRODUCT_ID)
            .expect("G923 PS mode switch should be registered");
        assert_eq!(g923_ps.expected_product_id_after_switch, G923_PRODUCT_ID);
        assert_eq!(g923_ps.switch_report[0], 0x30);
        assert_eq!(
            &g923_ps.switch_report[1..],
            &[0xf8, 0x09, 0x07, 0x01, 0x01, 0x00, 0x00]
        );
    }
}
