//! Parser for the Forza "Data Out" UDP telemetry packet.
//!
//! Forza Horizon 5/6 and Forza Motorsport all expose telemetry through the
//! same in-game "Data Out" feature. Three packet formats exist: Sled (232
//! bytes), Dash / Car Dash (Sled + more, 311 bytes), and Race. This parser
//! reads the Sled block unconditionally -- it has been byte-stable across
//! Forza titles for years and is what the LED shift-light output actually
//! needs (RPM, idle RPM, redline) -- and additionally reads the Dash block
//! when the packet is long enough to contain it.
//!
//! IMPORTANT: Titles occasionally append new fields to the *end* of the
//! Dash block (Forza Horizon 5, for example, is widely reported to add tire
//! wear fields after the base Dash block). The trailing `TireWearExt` read
//! here is a best-effort placeholder for that and is **not verified**
//! against a live capture. Before depending on it (or on exact `Gear`
//! semantics), run `sld-cli capture-forza` against your actual game/title
//! and confirm the bytes -- see docs/telemetry-protocol-forza.md.

const SLED_LEN: usize = 232;
const DASH_LEN: usize = 311; // SLED_LEN + Dash-specific fields
const DASH_TIRE_WEAR_LEN: usize = DASH_LEN + 16; // + 4 speculative trailing f32 fields

fn f32_at(buf: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap())
}
fn i32_at(buf: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap())
}
fn u32_at(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap())
}
fn u16_at(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(buf[offset..offset + 2].try_into().unwrap())
}

/// Fields present in every Data Out packet regardless of format.
#[derive(Debug, Clone, Default)]
pub struct SledData {
    pub is_race_on: bool,
    pub timestamp_ms: u32,
    pub engine_max_rpm: f32,
    pub engine_idle_rpm: f32,
    pub current_engine_rpm: f32,
    pub velocity: [f32; 3],
    pub car_ordinal: i32,
    pub car_class: i32,
    pub car_performance_index: i32,
    pub drivetrain_type: i32,
    pub num_cylinders: i32,
}

/// Present only when the game is configured to send the Dash (or Car Dash)
/// format, i.e. the packet is at least `DASH_LEN` bytes.
#[derive(Debug, Clone, Default)]
pub struct DashData {
    pub speed_mps: f32,
    pub power_watts: f32,
    pub torque_nm: f32,
    pub tire_temp: [f32; 4],
    pub boost: f32,
    pub fuel: f32,
    pub distance_traveled_m: f32,
    pub best_lap_s: f32,
    pub last_lap_s: f32,
    pub current_lap_s: f32,
    pub current_race_time_s: f32,
    pub lap_number: u16,
    pub race_position: u8,
    pub accel: u8,
    pub brake: u8,
    pub clutch: u8,
    pub hand_brake: u8,
    pub gear: u8,
    pub steer: i8,
    pub normalized_driving_line: i8,
    pub normalized_ai_brake_difference: i8,
}

/// Speculative trailing extension -- see module docs. Only populated when
/// the packet is long enough; absence doesn't mean the game didn't send it,
/// it may just live at a different offset than guessed here.
#[derive(Debug, Clone, Default)]
pub struct TireWearExt {
    pub front_left: f32,
    pub front_right: f32,
    pub rear_left: f32,
    pub rear_right: f32,
}

#[derive(Debug, Clone)]
pub struct ForzaPacket {
    pub sled: SledData,
    pub dash: Option<DashData>,
    pub tire_wear: Option<TireWearExt>,
}

/// Parse a raw UDP payload from Forza's Data Out feature.
///
/// Returns `None` if the buffer is shorter than the minimum Sled packet
/// size (i.e. clearly not a Data Out packet).
pub fn parse(buf: &[u8]) -> Option<ForzaPacket> {
    if buf.len() < SLED_LEN {
        return None;
    }

    let sled = SledData {
        is_race_on: i32_at(buf, 0) != 0,
        timestamp_ms: u32_at(buf, 4),
        engine_max_rpm: f32_at(buf, 8),
        engine_idle_rpm: f32_at(buf, 12),
        current_engine_rpm: f32_at(buf, 16),
        velocity: [f32_at(buf, 32), f32_at(buf, 36), f32_at(buf, 40)],
        car_ordinal: i32_at(buf, 212),
        car_class: i32_at(buf, 216),
        car_performance_index: i32_at(buf, 220),
        drivetrain_type: i32_at(buf, 224),
        num_cylinders: i32_at(buf, 228),
    };

    let dash = (buf.len() >= DASH_LEN).then(|| DashData {
        speed_mps: f32_at(buf, 244),
        power_watts: f32_at(buf, 248),
        torque_nm: f32_at(buf, 252),
        tire_temp: [
            f32_at(buf, 256),
            f32_at(buf, 260),
            f32_at(buf, 264),
            f32_at(buf, 268),
        ],
        boost: f32_at(buf, 272),
        fuel: f32_at(buf, 276),
        distance_traveled_m: f32_at(buf, 280),
        best_lap_s: f32_at(buf, 284),
        last_lap_s: f32_at(buf, 288),
        current_lap_s: f32_at(buf, 292),
        current_race_time_s: f32_at(buf, 296),
        lap_number: u16_at(buf, 300),
        race_position: buf[302],
        accel: buf[303],
        brake: buf[304],
        clutch: buf[305],
        hand_brake: buf[306],
        gear: buf[307],
        steer: buf[308] as i8,
        normalized_driving_line: buf[309] as i8,
        normalized_ai_brake_difference: buf[310] as i8,
    });

    let tire_wear = (buf.len() >= DASH_TIRE_WEAR_LEN).then(|| TireWearExt {
        front_left: f32_at(buf, DASH_LEN),
        front_right: f32_at(buf, DASH_LEN + 4),
        rear_left: f32_at(buf, DASH_LEN + 8),
        rear_right: f32_at(buf, DASH_LEN + 12),
    });

    Some(ForzaPacket {
        sled,
        dash,
        tire_wear,
    })
}

impl ForzaPacket {
    /// Convert into the app-wide normalized telemetry frame.
    pub fn to_telemetry_frame(&self, game: &str) -> sld_core::telemetry::TelemetryFrame {
        use sld_core::telemetry::TelemetryFrame;
        use std::collections::HashMap;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut extra = HashMap::new();
        extra.insert("car_ordinal".to_string(), self.sled.car_ordinal as f32);
        extra.insert("car_class".to_string(), self.sled.car_class as f32);
        extra.insert(
            "car_performance_index".to_string(),
            self.sled.car_performance_index as f32,
        );

        let (speed_mps, throttle, brake, clutch, steer, gear, fuel, lap, lap_time_s, position) =
            if let Some(d) = &self.dash {
                extra.insert("power_watts".to_string(), d.power_watts);
                extra.insert("torque_nm".to_string(), d.torque_nm);
                extra.insert("boost".to_string(), d.boost);
                extra.insert("tire_temp_fl".to_string(), d.tire_temp[0]);
                extra.insert("tire_temp_fr".to_string(), d.tire_temp[1]);
                extra.insert("tire_temp_rl".to_string(), d.tire_temp[2]);
                extra.insert("tire_temp_rr".to_string(), d.tire_temp[3]);
                (
                    d.speed_mps,
                    d.accel as f32 / 255.0,
                    d.brake as f32 / 255.0,
                    d.clutch as f32 / 255.0,
                    d.steer as f32 / 127.0,
                    d.gear as i8,
                    Some(d.fuel),
                    Some(d.lap_number as u32),
                    Some(d.current_lap_s),
                    Some(d.race_position as u32),
                )
            } else {
                // Sled-only packet: no pedal/gear telemetry available, fall
                // back to velocity magnitude for speed.
                let speed = (self.sled.velocity[0].powi(2)
                    + self.sled.velocity[1].powi(2)
                    + self.sled.velocity[2].powi(2))
                .sqrt();
                (speed, 0.0, 0.0, 0.0, 0.0, 0, None, None, None, None)
            };

        if let Some(tw) = &self.tire_wear {
            extra.insert("tire_wear_fl".to_string(), tw.front_left);
            extra.insert("tire_wear_fr".to_string(), tw.front_right);
            extra.insert("tire_wear_rl".to_string(), tw.rear_left);
            extra.insert("tire_wear_rr".to_string(), tw.rear_right);
        }

        TelemetryFrame {
            source: "forza".to_string(),
            game: Some(game.to_string()),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            is_running: self.sled.is_race_on,
            rpm: self.sled.current_engine_rpm,
            rpm_idle: self.sled.engine_idle_rpm,
            rpm_max: self.sled.engine_max_rpm,
            speed_mps,
            gear,
            throttle,
            brake,
            clutch,
            steer,
            fuel,
            lap,
            lap_time_s,
            position,
            extra,
        }
    }
}
