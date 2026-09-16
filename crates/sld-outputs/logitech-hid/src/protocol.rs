//! HID output-report encoding for Logitech wheel RPM shift-light LEDs.
//!
//! The G27/G29/Driving Force GT family exposes a row of 5 RPM LEDs driven
//! by an HID output report. The encoding used here for [`G29Protocol`]
//! matches the widely-cited community reverse-engineering of that report
//! (the same command implemented by the Linux `hid-lg4ff` kernel driver and
//! userspace tools such as `oversteer`): report id `0xf8`, sub-command
//! `0x12`, then a single byte bitmask of which of the 5 LEDs are lit (bit 0
//! = leftmost).
//!
//! G920/G923 (Xbox-bus wheels) are believed to use a related but not
//! identical report layout. [`G920Protocol`] is a **stub** pending
//! verification against real hardware -- see docs/led-hid-protocol.md
//! before relying on it.
//!
//! Adding a new wheel family means adding one more `WheelLedProtocol` impl
//! here and registering it in [`all_known_protocols`] -- nothing else in
//! the crate needs to change.

pub trait WheelLedProtocol: Send + Sync {
    /// Human readable id, e.g. "g29".
    fn id(&self) -> &'static str;

    /// USB vendor/product IDs this protocol applies to.
    fn matches(&self, vendor_id: u16, product_id: u16) -> bool;

    /// Build the raw HID report bytes for a given 5-bit LED mask (bit 0 =
    /// leftmost LED).
    fn encode_leds(&self, led_bits: u8) -> Vec<u8>;
}

pub const LOGITECH_VENDOR_ID: u16 = 0x046d;

pub struct G29Protocol;

impl WheelLedProtocol for G29Protocol {
    fn id(&self) -> &'static str {
        "g29"
    }

    fn matches(&self, vendor_id: u16, product_id: u16) -> bool {
        // G27 = 0xc29b, G29 = 0xc24f, Driving Force GT = 0xc29a.
        vendor_id == LOGITECH_VENDOR_ID && matches!(product_id, 0xc24f | 0xc29b | 0xc29a)
    }

    fn encode_leds(&self, led_bits: u8) -> Vec<u8> {
        vec![0xf8, 0x12, led_bits & 0x1f, 0x00, 0x00, 0x00, 0x00]
    }
}

/// Placeholder for the G920 (Xbox One) / G923 protocol. **Unverified** --
/// currently mirrors the G29 command as a starting point only. Capture the
/// real output report (e.g. by sniffing G HUB with Wireshark + USBPcap, or
/// referencing `hid-lg4ff.c` if/when it grows support) before relying on
/// this for real LED output.
pub struct G920Protocol;

impl WheelLedProtocol for G920Protocol {
    fn id(&self) -> &'static str {
        "g920"
    }

    fn matches(&self, vendor_id: u16, product_id: u16) -> bool {
        // G920 = 0xc262, G923 (Xbox) = 0xc267, G923 (PlayStation) = 0xc266.
        vendor_id == LOGITECH_VENDOR_ID && matches!(product_id, 0xc262 | 0xc266 | 0xc267)
    }

    fn encode_leds(&self, led_bits: u8) -> Vec<u8> {
        vec![0xf8, 0x12, led_bits & 0x1f, 0x00, 0x00, 0x00, 0x00]
    }
}

pub fn all_known_protocols() -> Vec<Box<dyn WheelLedProtocol>> {
    vec![Box::new(G29Protocol), Box::new(G920Protocol)]
}
