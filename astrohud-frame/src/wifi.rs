//! Local Wi-Fi link state. This deliberately makes no claim about internet access.
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiState {
    Connected,
    Offline,
    Unknown,
}

pub fn current() -> WifiState {
    from_sysfs(Path::new("/sys/class/net"))
}

fn from_sysfs(root: &Path) -> WifiState {
    let Ok(entries) = fs::read_dir(root) else {
        return WifiState::Unknown;
    };
    let mut found = false;
    let mut unknown = false;
    for entry in entries {
        let Ok(entry) = entry else {
            unknown = true;
            continue;
        };
        let path = entry.path();
        if !path.join("wireless").is_dir() {
            continue;
        }
        found = true;
        match fs::read_to_string(path.join("carrier"))
            .as_deref()
            .map(str::trim)
        {
            Ok("1") => return WifiState::Connected,
            Ok("0") => (),
            // Reading carrier while an interface is down may return EINVAL.
            _ => match fs::read_to_string(path.join("operstate"))
                .as_deref()
                .map(str::trim)
            {
                Ok("down" | "notpresent" | "lowerlayerdown") => (),
                _ => unknown = true,
            },
        }
    }
    if found && !unknown {
        WifiState::Offline
    } else {
        WifiState::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinguishes_wifi_from_ethernet_and_unreadable_state() {
        let root = std::env::temp_dir().join(format!("wifi-state-{}", rand::random::<u64>()));
        fs::create_dir_all(root.join("eth0")).unwrap();
        fs::write(root.join("eth0/carrier"), "1\n").unwrap();
        assert_eq!(from_sysfs(&root), WifiState::Unknown);
        fs::create_dir_all(root.join("wlan0/wireless")).unwrap();
        fs::write(root.join("wlan0/carrier"), "0\n").unwrap();
        assert_eq!(from_sysfs(&root), WifiState::Offline);
        fs::write(root.join("wlan0/carrier"), "1\n").unwrap();
        assert_eq!(from_sysfs(&root), WifiState::Connected);
        fs::remove_file(root.join("wlan0/carrier")).unwrap();
        assert_eq!(from_sysfs(&root), WifiState::Unknown);
        fs::write(root.join("wlan0/operstate"), "down\n").unwrap();
        assert_eq!(from_sysfs(&root), WifiState::Offline);
        fs::remove_dir_all(root).unwrap();
    }
}
