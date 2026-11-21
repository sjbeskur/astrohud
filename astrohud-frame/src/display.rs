//! Best-effort display detection. Unknown/stale signals always keep playback active.
//! CEC sends only Give Device Power Status, never wake/standby/input commands.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IdleMode {
    #[default]
    Off,
    Observe,
    Auto,
}
impl std::str::FromStr for IdleMode {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "observe" => Ok(Self::Observe),
            "auto" => Ok(Self::Auto),
            _ => Err("display idle mode must be off, observe or auto".into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Power {
    On,
    Standby,
    Disconnected,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Observation {
    pub power: Power,
    pub source: &'static str,
    #[serde(skip)]
    pub sampled: Instant,
}
impl Observation {
    fn new(power: Power, source: &'static str) -> Self {
        Self {
            power,
            source,
            sampled: Instant::now(),
        }
    }
}

/// Debounce positive off observations; recover immediately on on/unknown/stale.
#[derive(Default)]
pub struct IdlePolicy {
    off_since: Option<Instant>,
}
impl IdlePolicy {
    pub fn idle(&mut self, sample: Observation, now: Instant) -> bool {
        let fresh = now.saturating_duration_since(sample.sampled) <= Duration::from_secs(10);
        if !fresh || !matches!(sample.power, Power::Standby | Power::Disconnected) {
            self.off_since = None;
            return false;
        }
        let since = self.off_since.get_or_insert(now);
        now.saturating_duration_since(*since) >= Duration::from_secs(5)
    }
}

/// Preserve the remaining slide time, including settings changes received idle.
#[derive(Default)]
pub struct PauseTimer {
    remaining: Option<Duration>,
}
impl PauseTimer {
    pub fn pause(&mut self, deadline: Instant, now: Instant) {
        self.remaining
            .get_or_insert(deadline.saturating_duration_since(now));
    }
    pub fn reschedule_if_paused(&mut self, interval: Duration) -> bool {
        if let Some(remaining) = self.remaining.as_mut() {
            *remaining = interval;
            true
        } else {
            false
        }
    }
    pub fn resume(&mut self, now: Instant) -> Option<Instant> {
        self.remaining.take().map(|remaining| now + remaining)
    }
}

pub struct DisplayWatch {
    mode: IdleMode,
    latest: Option<Arc<Mutex<Observation>>>,
    policy: IdlePolicy,
    reported: Option<bool>,
}
impl DisplayWatch {
    pub fn new(mode: IdleMode) -> Self {
        let latest = if mode == IdleMode::Off {
            None
        } else {
            let shared = Arc::new(Mutex::new(Observation::new(Power::Unknown, "starting")));
            let output = Arc::clone(&shared);
            std::thread::spawn(move || {
                let mut probe = Probe::default();
                let mut previous = None;
                loop {
                    let sample = probe.read();
                    if previous != Some((sample.power, sample.source)) {
                        eprintln!(
                            "display observation: power={:?}, source={}",
                            sample.power, sample.source
                        );
                        previous = Some((sample.power, sample.source));
                    }
                    *output.lock().unwrap_or_else(|e| e.into_inner()) = sample;
                    std::thread::sleep(Duration::from_secs(2));
                }
            });
            Some(shared)
        };
        Self {
            mode,
            latest,
            policy: IdlePolicy::default(),
            reported: None,
        }
    }
    pub fn paused(&mut self) -> bool {
        let Some(shared) = &self.latest else {
            return false;
        };
        let sample = *shared.lock().unwrap_or_else(|e| e.into_inner());
        let idle = self.policy.idle(sample, Instant::now());
        if self.reported != Some(idle) {
            eprintln!(
                "display idle policy: mode={:?}, would_idle={idle}",
                self.mode
            );
            self.reported = Some(idle);
        }
        self.mode == IdleMode::Auto && idle
    }
}

#[derive(Default)]
pub struct Probe {
    #[cfg(target_os = "linux")]
    cec: Option<linux::Cec>,
}
impl Probe {
    pub fn read(&mut self) -> Observation {
        let connectors = hdmi_states(Path::new("/sys/class/drm"));
        if !connectors.is_empty() && connectors.iter().all(|s| s == "disconnected") {
            return Observation::new(Power::Disconnected, "hdmi_hotplug");
        }
        // Avoid attributing the state of one TV to a different output.
        if connectors.len() != 1 || connectors[0] != "connected" {
            return Observation::new(Power::Unknown, "hdmi_unavailable_or_ambiguous");
        }
        #[cfg(target_os = "linux")]
        {
            if self.cec.is_none() {
                self.cec = linux::Cec::open_single();
            }
            if let Some(cec) = &mut self.cec {
                match cec.power() {
                    Ok((power, source)) => return Observation::new(power, source),
                    Err(_) => self.cec = None,
                }
            }
        }
        Observation::new(Power::Unknown, "cec_unavailable")
    }
}
fn hdmi_states(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return vec![];
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains("-HDMI-"))
        .map(|e| {
            fs::read_to_string(e.path().join("status"))
                .unwrap_or_else(|_| "unknown".into())
                .trim()
                .to_owned()
        })
        .collect()
}

#[cfg(target_os = "linux")]
mod linux {
    use super::Power;
    use std::{
        fs::{self, File, OpenOptions},
        io,
        os::fd::AsRawFd,
    };
    // Linux CEC UAPI: include/uapi/linux/cec.h. Layout shared by x86_64/aarch64.
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct Message {
        tx_ts: u64,
        rx_ts: u64,
        len: u32,
        timeout: u32,
        sequence: u32,
        flags: u32,
        msg: [u8; 16],
        reply: u8,
        rx_status: u8,
        tx_status: u8,
        arb: u8,
        nack: u8,
        low: u8,
        error: u8,
    }
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct Addresses {
        addr: [u8; 4],
        mask: u16,
        version: u8,
        count: u8,
        vendor: u32,
        flags: u32,
        name: [u8; 15],
        primary: [u8; 4],
        kind: [u8; 4],
        all: [u8; 4],
        features: [[u8; 12]; 4],
    }
    #[repr(C)]
    #[derive(Default)]
    struct Caps {
        driver: [u8; 32],
        name: [u8; 32],
        available: u32,
        bits: u32,
        version: u32,
    }
    const fn request<T>(direction: u64, nr: u64) -> libc::c_ulong {
        ((direction << 30) | ((std::mem::size_of::<T>() as u64) << 16) | (b'a' as u64) << 8 | nr)
            as libc::c_ulong
    }
    const GET_CAPS: libc::c_ulong = request::<Caps>(3, 0);
    const GET_PHYSICAL: libc::c_ulong = request::<u16>(2, 1);
    const GET_ADDRESSES: libc::c_ulong = request::<Addresses>(2, 3);
    const SET_ADDRESSES: libc::c_ulong = request::<Addresses>(3, 4);
    const TRANSMIT: libc::c_ulong = request::<Message>(3, 5);
    const SET_MODE: libc::c_ulong = request::<u32>(1, 9);
    fn ioctl<T>(file: &File, request: libc::c_ulong, data: &mut T) -> io::Result<()> {
        // SAFETY: each request below uses its matching repr(C) UAPI type. Kernel
        // bounds the transfer by that type's size; data remains alive/exclusive.
        let result = unsafe { libc::ioctl(file.as_raw_fd(), request, data as *mut T) };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    pub struct Cec {
        file: File,
    }
    impl Cec {
        pub fn open_single() -> Option<Self> {
            let devices: Vec<_> = fs::read_dir("/dev")
                .ok()?
                .filter_map(Result::ok)
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name.strip_prefix("cec")
                        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                })
                .collect();
            if devices.len() != 1 {
                return None;
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(devices[0].path())
                .ok()?;
            let mut caps = Caps::default();
            ioctl(&file, GET_CAPS, &mut caps).ok()?;
            if caps.bits & 4 == 0 {
                return None;
            }
            let mut mode = 1u32; // shared initiator, no remote-control follower
            ioctl(&file, SET_MODE, &mut mode).ok()?;
            Some(Self { file })
        }
        pub fn power(&mut self) -> io::Result<(Power, &'static str)> {
            let mut physical = 0xffffu16;
            ioctl(&self.file, GET_PHYSICAL, &mut physical)?;
            if physical == 0xffff {
                return Ok((Power::Unknown, "cec_no_physical_address"));
            }
            let mut addresses = Addresses::default();
            ioctl(&self.file, GET_ADDRESSES, &mut addresses)?;
            if addresses.count == 0 {
                // Claim a playback address only on an unconfigured adapter.
                addresses.version = 5;
                addresses.count = 1;
                addresses.vendor = u32::MAX;
                addresses.primary[0] = 4;
                addresses.kind[0] = 3;
                addresses.all[0] = 0x10;
                addresses.name[..11].copy_from_slice(b"Memory Lane");
                ioctl(&self.file, SET_ADDRESSES, &mut addresses)?;
            }
            let Some(initiator) = addresses
                .addr
                .iter()
                .copied()
                .take(addresses.count.min(4) as usize)
                .find(|a| *a > 0 && *a < 15 && addresses.mask & (1u16 << a) != 0)
            else {
                return Ok((Power::Unknown, "cec_no_logical_address"));
            };
            let mut message = Message {
                len: 2,
                timeout: 1000,
                reply: 0x90,
                ..Default::default()
            };
            message.msg[0] = initiator << 4;
            message.msg[1] = 0x8f;
            ioctl(&self.file, TRANSMIT, &mut message)?;
            let power = decode(&message, initiator);
            Ok((
                power,
                if power == Power::Unknown {
                    "cec_no_definite_power_reply"
                } else {
                    "cec_power_status"
                },
            ))
        }
    }
    pub(super) fn decode(m: &Message, initiator: u8) -> Power {
        if m.tx_status & 1 == 0
            || m.rx_status & 1 == 0
            || m.rx_status & 4 != 0
            || m.reply != 0x90
            || m.len != 3
            || m.msg[0] >> 4 != 0
            || m.msg[0] & 15 != initiator
            || m.msg[1] != 0x90
        {
            return Power::Unknown;
        }
        match m.msg[2] {
            0 => Power::On,
            1 => Power::Standby,
            _ => Power::Unknown,
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn kernel_layout_and_power_reply_validation() {
            assert_eq!(std::mem::size_of::<Message>(), 56);
            assert_eq!(std::mem::size_of::<Addresses>(), 92);
            assert_eq!(std::mem::offset_of!(Addresses, features), 43);
            assert_eq!(GET_CAPS, 0xc04c6100);
            assert_eq!(GET_PHYSICAL, 0x80026101);
            assert_eq!(GET_ADDRESSES, 0x805c6103);
            assert_eq!(SET_ADDRESSES, 0xc05c6104);
            assert_eq!(SET_MODE, 0x40046109);
            assert_eq!(TRANSMIT, 0xc0386105);
            let mut m = Message {
                len: 3,
                reply: 0x90,
                tx_status: 1,
                rx_status: 1,
                ..Default::default()
            };
            m.msg[..3].copy_from_slice(&[0x04, 0x90, 1]);
            assert_eq!(decode(&m, 4), Power::Standby);
            m.msg[2] = 0;
            assert_eq!(decode(&m, 4), Power::On);
            for status in [2, 3, 255] {
                m.msg[2] = status;
                assert_eq!(decode(&m, 4), Power::Unknown)
            }
            m.msg[2] = 1;
            m.rx_status = 2;
            assert_eq!(decode(&m, 4), Power::Unknown);
            m.rx_status = 1;
            m.msg[0] = 0x14;
            assert_eq!(decode(&m, 4), Power::Unknown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn debounce_unknown_stale_and_wake() {
        let start = Instant::now();
        let mut p = IdlePolicy::default();
        let sample = |power, secs| Observation {
            power,
            source: "test",
            sampled: start + Duration::from_secs(secs),
        };
        assert!(!p.idle(sample(Power::Standby, 0), start));
        assert!(!p.idle(sample(Power::Standby, 4), start + Duration::from_secs(4)));
        assert!(p.idle(sample(Power::Standby, 6), start + Duration::from_secs(6)));
        assert!(!p.idle(sample(Power::Unknown, 7), start + Duration::from_secs(7)));
        assert!(!p.idle(
            sample(Power::Disconnected, 8),
            start + Duration::from_secs(8)
        ));
        assert!(p.idle(
            sample(Power::Disconnected, 14),
            start + Duration::from_secs(14)
        ));
        assert!(!p.idle(
            sample(Power::Disconnected, 14),
            start + Duration::from_secs(25)
        ));
        assert!(!p.idle(sample(Power::On, 26), start + Duration::from_secs(26)));
    }
    #[test]
    fn pause_preserves_slide_time_and_accepts_new_interval_while_hidden() {
        let start = Instant::now();
        let mut timer = PauseTimer::default();
        assert!(!timer.reschedule_if_paused(Duration::from_secs(12)));
        timer.pause(
            start + Duration::from_secs(12),
            start + Duration::from_secs(5),
        );
        timer.pause(
            start + Duration::from_secs(12),
            start + Duration::from_secs(30),
        );
        assert_eq!(
            timer.resume(start + Duration::from_secs(60)),
            Some(start + Duration::from_secs(67))
        );
        timer.pause(
            start + Duration::from_secs(80),
            start + Duration::from_secs(70),
        );
        assert!(timer.reschedule_if_paused(Duration::from_secs(20)));
        assert_eq!(
            timer.resume(start + Duration::from_secs(300)),
            Some(start + Duration::from_secs(320))
        );
        timer.pause(
            start + Duration::from_secs(320),
            start + Duration::from_secs(301),
        );
        assert!(timer.reschedule_if_paused(Duration::ZERO));
        assert_eq!(
            timer.resume(start + Duration::from_secs(900)),
            Some(start + Duration::from_secs(900))
        );
    }

    #[test]
    fn missing_connector_and_invalid_mode_are_safe() {
        assert!(hdmi_states(Path::new("/no/such/drm")).is_empty());
        assert!("pretend-off".parse::<IdleMode>().is_err());
        assert!("observe".parse::<IdleMode>().is_ok());
    }
}
