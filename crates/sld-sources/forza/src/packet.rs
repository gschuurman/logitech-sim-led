//! Parser for the Forza "Data Out" UDP telemetry packet.
//!
//! Byte layout verified against Forza's own documentation:
//! - Forza Horizon 6: <https://support.forza.net/hc/en-us/articles/51744149102611-Forza-Horizon-6-Data-Out-Documentation>
//!   (Forza Horizon 5's Car Dash payload is documented elsewhere as
//!   byte-for-byte identical to Horizon 6's.)
//! - Forza Motorsport (2023): <https://support.forza.net/hc/en-us/articles/21742934024211-Forza-Motorsport-Data-Out-Documentation>
//!
//! There are two genuinely different layouts, not one layout with an
//! optional trailing extension as earlier revisions of this parser assumed:
//!
//! - **Forza Motorsport** lets the player pick "Sled" (232 bytes) or "Dash"
//!   (232-byte Sled prefix, then Position/Speed/.../pedals/Gear, then
//!   `TireWearFrontLeft..RearRight` and `TrackOrdinal` -- 331 bytes).
//! - **Forza Horizon 5/6** always send a single fixed 324-byte packet: the
//!   same 232-byte Sled prefix, then **`CarGroup`/`SmashableVelDiff`/
//!   `SmashableMass`** (fields Motorsport doesn't send), *then* the same
//!   Position/Speed/.../Gear block -- but Horizon does **not** send
//!   `TireWear*`/`TrackOrdinal` (fields Motorsport does send). So the
//!   Position/Speed/etc. block sits at a different byte offset per title.
//!
//! The Sled prefix itself (RPM, velocity, car ids -- everything the LED
//! shift-light output needs) is byte-identical across both titles and
//! unaffected by this split, since the divergence only starts after it.
//!
//! Packet length reliably tells the two apart: Horizon's Dash is always
//! exactly 324 bytes, while Motorsport's is always >= 331 (base fields +
//! TireWear + TrackOrdinal) -- well clear of 324. See `parse` below.

const SLED_LEN: usize = 232;

/// Length of the Position..NormalizedAIBrakeDifference block shared by both
/// titles' Dash format, once you're at the right starting offset for each.
const DASH_TAIL_LEN: usize = 79;

/// Forza Horizon 5/6: fixed total packet size (confirmed by FH6's docs:
/// "Total packet size: 324 bytes").
const FH_DASH_LEN: usize = 324;
/// Forza Horizon 5/6: the 3 extra fields (CarGroup, SmashableVelDiff,
/// SmashableMass) sit right after the Sled prefix, before the shared tail.
const FH_TAIL_OFFSET: usize = SLED_LEN + 12;

/// Forza Motorsport: shared tail starts immediately after the Sled prefix.
const FM_TAIL_OFFSET: usize = SLED_LEN;
/// Forza Motorsport: where TireWear starts, right after the shared tail.
const FM_TIRE_WEAR_OFFSET: usize = SLED_LEN + DASH_TAIL_LEN; // 311
/// Forza Motorsport: where TrackOrdinal starts, right after TireWear.
const FM_TRACK_ORDINAL_OFFSET: usize = FM_TIRE_WEAR_OFFSET + 16; // 327
/// Forza Motorsport: minimum full Dash packet size (Sled + tail + TireWear +
/// TrackOrdinal). The docs don't state whether the wire format adds trailing
/// alignment padding (Horizon's does, by 1 byte), so this is a lower bound
/// checked with `>=`, not an exact size.
const FM_DASH_MIN_LEN: usize = FM_TRACK_ORDINAL_OFFSET + 4; // 331

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

/// Fields present in every Data Out packet regardless of format -- the
/// Sled prefix. Byte-identical across Forza Horizon 5/6 and Forza
/// Motorsport.
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

fn parse_sled(buf: &[u8]) -> SledData {
    SledData {
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
    }
}

/// The Position/Speed/.../Gear block shared by both titles' Dash format
/// (`docs/telemetry-protocol-forza.md` has the field-by-field offsets).
/// `Gear`'s exact Reverse/Neutral convention isn't specified by Forza's own
/// docs -- treat it as a raw value, see that doc's note before assuming a
/// transform.
#[derive(Debug, Clone, Default)]
pub struct DashData {
    pub position: [f32; 3],
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

/// Parses the shared 79-byte Dash tail starting at `off`, which is a
/// *different* absolute offset per title -- see the module docs.
fn parse_dash_tail(buf: &[u8], off: usize) -> DashData {
    DashData {
        position: [f32_at(buf, off), f32_at(buf, off + 4), f32_at(buf, off + 8)],
        speed_mps: f32_at(buf, off + 12),
        power_watts: f32_at(buf, off + 16),
        torque_nm: f32_at(buf, off + 20),
        tire_temp: [
            f32_at(buf, off + 24),
            f32_at(buf, off + 28),
            f32_at(buf, off + 32),
            f32_at(buf, off + 36),
        ],
        boost: f32_at(buf, off + 40),
        fuel: f32_at(buf, off + 44),
        distance_traveled_m: f32_at(buf, off + 48),
        best_lap_s: f32_at(buf, off + 52),
        last_lap_s: f32_at(buf, off + 56),
        current_lap_s: f32_at(buf, off + 60),
        current_race_time_s: f32_at(buf, off + 64),
        lap_number: u16_at(buf, off + 68),
        race_position: buf[off + 70],
        accel: buf[off + 71],
        brake: buf[off + 72],
        clutch: buf[off + 73],
        hand_brake: buf[off + 74],
        gear: buf[off + 75],
        steer: buf[off + 76] as i8,
        normalized_driving_line: buf[off + 77] as i8,
        normalized_ai_brake_difference: buf[off + 78] as i8,
    }
}

#[derive(Debug, Clone, Default)]
pub struct TireWearExt {
    pub front_left: f32,
    pub front_right: f32,
    pub rear_left: f32,
    pub rear_right: f32,
}

/// Fields that exist in exactly one title's Dash format, per Forza's docs.
#[derive(Debug, Clone)]
pub enum TitleExtras {
    /// Packet was Sled-only -- neither title's extra fields are present.
    None,
    /// Forza Horizon 5/6-only fields.
    Horizon {
        car_group: u32,
        smashable_vel_diff: f32,
        smashable_mass: f32,
    },
    /// Forza Motorsport-only fields.
    Motorsport {
        tire_wear: TireWearExt,
        track_ordinal: i32,
    },
}

#[derive(Debug, Clone)]
pub struct ForzaPacket {
    pub sled: SledData,
    pub dash: Option<DashData>,
    pub extras: TitleExtras,
}

/// Parse a raw UDP payload from Forza's Data Out feature. Returns `None` if
/// the buffer is shorter than the minimum Sled packet size.
pub fn parse(buf: &[u8]) -> Option<ForzaPacket> {
    if buf.len() < SLED_LEN {
        return None;
    }
    let sled = parse_sled(buf);

    // Check Motorsport's (longer) size first -- its minimum, 331 bytes, is
    // safely past Horizon's fixed 324, so there's no overlap to worry
    // about between the two `>=` checks below.
    if buf.len() >= FM_DASH_MIN_LEN {
        let dash = parse_dash_tail(buf, FM_TAIL_OFFSET);
        let tire_wear = TireWearExt {
            front_left: f32_at(buf, FM_TIRE_WEAR_OFFSET),
            front_right: f32_at(buf, FM_TIRE_WEAR_OFFSET + 4),
            rear_left: f32_at(buf, FM_TIRE_WEAR_OFFSET + 8),
            rear_right: f32_at(buf, FM_TIRE_WEAR_OFFSET + 12),
        };
        let track_ordinal = i32_at(buf, FM_TRACK_ORDINAL_OFFSET);
        return Some(ForzaPacket {
            sled,
            dash: Some(dash),
            extras: TitleExtras::Motorsport {
                tire_wear,
                track_ordinal,
            },
        });
    }

    if buf.len() >= FH_DASH_LEN {
        let car_group = u32_at(buf, SLED_LEN);
        let smashable_vel_diff = f32_at(buf, SLED_LEN + 4);
        let smashable_mass = f32_at(buf, SLED_LEN + 8);
        let dash = parse_dash_tail(buf, FH_TAIL_OFFSET);
        return Some(ForzaPacket {
            sled,
            dash: Some(dash),
            extras: TitleExtras::Horizon {
                car_group,
                smashable_vel_diff,
                smashable_mass,
            },
        });
    }

    if buf.len() >= SLED_LEN + DASH_TAIL_LEN {
        // 311 bytes: Motorsport's shared tail without TireWear/TrackOrdinal.
        // Not a documented standalone wire format, but parsed defensively
        // in case a future/older build trims those fields.
        let dash = parse_dash_tail(buf, FM_TAIL_OFFSET);
        return Some(ForzaPacket {
            sled,
            dash: Some(dash),
            extras: TitleExtras::None,
        });
    }

    Some(ForzaPacket {
        sled,
        dash: None,
        extras: TitleExtras::None,
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

        match &self.extras {
            TitleExtras::Horizon {
                car_group,
                smashable_vel_diff,
                smashable_mass,
            } => {
                extra.insert("car_group".to_string(), *car_group as f32);
                extra.insert("smashable_vel_diff".to_string(), *smashable_vel_diff);
                extra.insert("smashable_mass".to_string(), *smashable_mass);
            }
            TitleExtras::Motorsport {
                tire_wear,
                track_ordinal,
            } => {
                extra.insert("tire_wear_fl".to_string(), tire_wear.front_left);
                extra.insert("tire_wear_fr".to_string(), tire_wear.front_right);
                extra.insert("tire_wear_rl".to_string(), tire_wear.rear_left);
                extra.insert("tire_wear_rr".to_string(), tire_wear.rear_right);
                extra.insert("track_ordinal".to_string(), *track_ordinal as f32);
            }
            TitleExtras::None => {}
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sled_only_packet() {
        let mut buf = vec![0u8; SLED_LEN];
        buf[0..4].copy_from_slice(&1i32.to_le_bytes()); // IsRaceOn
        buf[8..12].copy_from_slice(&7000.0f32.to_le_bytes()); // EngineMaxRpm
        buf[12..16].copy_from_slice(&1000.0f32.to_le_bytes()); // EngineIdleRpm
        buf[16..20].copy_from_slice(&4500.0f32.to_le_bytes()); // CurrentEngineRpm

        let pkt = parse(&buf).expect("should parse");
        assert!(pkt.sled.is_race_on);
        assert_eq!(pkt.sled.engine_max_rpm, 7000.0);
        assert_eq!(pkt.sled.engine_idle_rpm, 1000.0);
        assert_eq!(pkt.sled.current_engine_rpm, 4500.0);
        assert!(pkt.dash.is_none());
    }

    #[test]
    fn parses_horizon_fixed_dash_packet() {
        let mut buf = vec![0u8; FH_DASH_LEN];
        buf[16..20].copy_from_slice(&5500.0f32.to_le_bytes()); // CurrentEngineRpm
        buf[SLED_LEN..SLED_LEN + 4].copy_from_slice(&42u32.to_le_bytes()); // CarGroup
        let tail = FH_TAIL_OFFSET;
        buf[tail + 12..tail + 16].copy_from_slice(&55.0f32.to_le_bytes()); // Speed
        buf[tail + 75] = 3; // Gear

        let pkt = parse(&buf).expect("should parse");
        assert_eq!(pkt.sled.current_engine_rpm, 5500.0);
        let dash = pkt.dash.expect("dash present");
        assert_eq!(dash.speed_mps, 55.0);
        assert_eq!(dash.gear, 3);
        match pkt.extras {
            TitleExtras::Horizon { car_group, .. } => assert_eq!(car_group, 42),
            _ => panic!("expected Horizon extras"),
        }
    }

    #[test]
    fn parses_motorsport_dash_packet_with_tire_wear_and_track_ordinal() {
        let mut buf = vec![0u8; FM_DASH_MIN_LEN];
        let tail = FM_TAIL_OFFSET;
        buf[tail + 12..tail + 16].copy_from_slice(&40.0f32.to_le_bytes()); // Speed
        buf[FM_TIRE_WEAR_OFFSET..FM_TIRE_WEAR_OFFSET + 4].copy_from_slice(&0.2f32.to_le_bytes());
        buf[FM_TRACK_ORDINAL_OFFSET..FM_TRACK_ORDINAL_OFFSET + 4]
            .copy_from_slice(&9i32.to_le_bytes());

        let pkt = parse(&buf).expect("should parse");
        let dash = pkt.dash.expect("dash present");
        assert_eq!(dash.speed_mps, 40.0);
        match pkt.extras {
            TitleExtras::Motorsport {
                tire_wear,
                track_ordinal,
            } => {
                assert_eq!(tire_wear.front_left, 0.2);
                assert_eq!(track_ordinal, 9);
            }
            _ => panic!("expected Motorsport extras"),
        }
    }
}
