use crate::{
    Result,
    store::{FRAME, PROVISIONER, Pending, State, Store},
};
use std::{fs, path::Path, process::Command, thread, time::Duration};

const UNITS: [&str; 2] = ["astrohud-frame.service", "astrohud-provisioner.service"];

pub trait Services {
    fn stop(&self) -> Result<()>;
    fn start(&self) -> Result<()>;
    fn healthy(&self, release: &Path) -> Result<()>;
}

pub struct Systemd;

fn systemctl(args: &[&str]) -> Result<String> {
    let output = Command::new("/usr/bin/systemctl")
        .arg("--no-ask-password")
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "systemctl {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

impl Services for Systemd {
    fn stop(&self) -> Result<()> {
        systemctl(&["stop", UNITS[0], UNITS[1]])?;
        Ok(())
    }
    fn start(&self) -> Result<()> {
        systemctl(&["reset-failed", UNITS[0], UNITS[1]])?;
        systemctl(&["start", UNITS[0], UNITS[1]])?;
        Ok(())
    }
    fn healthy(&self, release: &Path) -> Result<()> {
        for (unit, binary) in UNITS.iter().zip([FRAME, PROVISIONER]) {
            let output = systemctl(&[
                "show",
                unit,
                "--property=ActiveState,SubState,MainPID,NRestarts",
            ])?;
            let field = |name: &str| -> Result<&str> {
                output
                    .lines()
                    .find_map(|line| line.strip_prefix(name))
                    .ok_or_else(|| format!("missing service field {name}").into())
            };
            if field("ActiveState=")? != "active"
                || field("SubState=")? != "running"
                || field("NRestarts=")? != "0"
            {
                return Err(format!("{unit} is not stably running (or has restarted)").into());
            }
            let pid: u32 = field("MainPID=")?.parse()?;
            if pid == 0
                || fs::read_link(format!("/proc/{pid}/exe"))?
                    != fs::canonicalize(release.join(binary))?
            {
                return Err(format!("{unit} is not running the selected binary").into());
            }
        }
        Ok(())
    }
}

pub fn safe_to_update() -> Result<()> {
    for marker in [
        "/var/lib/astrohud/reset-in-progress",
        "/var/lib/astrohud/provisioning-required",
    ] {
        if Path::new(marker).exists() {
            return Err("finish factory reset or Wi-Fi setup before updating".into());
        }
    }
    if !Path::new("/etc/NetworkManager/system-connections/astrohud-wifi.nmconnection").exists() {
        return Err("configure household Wi-Fi before updating".into());
    }
    Ok(())
}

pub fn activate(
    store: &Store,
    state: &mut State,
    to: &str,
    services: &impl Services,
    samples: u32,
    interval: Duration,
) -> Result<()> {
    if state.pending.is_some() {
        return Err("interrupted update pending; run recover first".into());
    }
    store.ensure_current(state)?;
    let from = state.current.clone().ok_or("updater is not initialized")?;
    if from == to {
        return Err("selected release is already installed".into());
    }
    // Persist intent BEFORE stopping anything or switching the one shared link.
    state.pending = Some(Pending {
        from: from.clone(),
        to: to.into(),
    });
    store.save(state)?;
    let attempt = (|| -> Result<()> {
        services.stop()?;
        store.switch(to)?;
        services.start()?;
        for _ in 0..samples {
            thread::sleep(interval);
            services.healthy(&store.release_dir(to)?)?;
        }
        // A failed commit is also an interrupted transaction: roll it back.
        let mut committed = state.clone();
        committed.previous = Some(from.clone());
        committed.current = Some(to.into());
        committed.pending = None;
        committed.last_result = Some(format!("Installed {to}"));
        store.save(&committed)?;
        *state = committed;
        Ok(())
    })();
    if let Err(error) = attempt {
        // Keep the durable journal until the old services pass health checks.
        let recovery = (|| -> Result<()> {
            services.stop()?;
            store.switch(&from)?;
            services.start()?;
            thread::sleep(interval);
            services.healthy(&store.release_dir(&from)?)?;
            state.pending = None;
            state.last_result = Some(format!("Update to {to} failed; restored {from}: {error}"));
            store.save(state)
        })();
        return match recovery {
            Ok(()) => Err(format!("update failed; restored {from}: {error}").into()),
            Err(recovery) => Err(format!("update failed: {error}; rollback needs recovery: {recovery}. Run `sudo memory-lane recover` or reboot.").into()),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Fake {
        fail: bool,
        starts: Cell<u32>,
    }
    impl Services for Fake {
        fn stop(&self) -> Result<()> {
            Ok(())
        }
        fn start(&self) -> Result<()> {
            self.starts.set(self.starts.get() + 1);
            Ok(())
        }
        fn healthy(&self, _: &Path) -> Result<()> {
            if self.fail && self.starts.get() == 1 {
                Err("simulated crash".into())
            } else {
                Ok(())
            }
        }
    }
    fn setup() -> (tempfile::TempDir, Store, State) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            root: tmp.path().join("store"),
        };
        let _lock = store.lock().unwrap();
        let frame = tmp.path().join(FRAME);
        let provisioner = tmp.path().join(PROVISIONER);
        fs::write(&frame, b"old frame").unwrap();
        fs::write(&provisioner, b"old provisioner").unwrap();
        store.init(&frame, &provisioner).unwrap();
        let next = store.release_dir("next").unwrap();
        fs::create_dir(&next).unwrap();
        fs::write(next.join(FRAME), b"new frame").unwrap();
        fs::write(next.join(PROVISIONER), b"new provisioner").unwrap();
        let state = store.state().unwrap();
        (tmp, store, state)
    }
    #[test]
    fn successful_switch_and_manual_rollback() {
        let (_tmp, store, mut state) = setup();
        let services = Fake {
            fail: false,
            starts: Cell::new(0),
        };
        activate(&store, &mut state, "next", &services, 2, Duration::ZERO).unwrap();
        assert_eq!(state.current.as_deref(), Some("next"));
        assert_eq!(state.previous.as_deref(), Some("baseline"));
        activate(&store, &mut state, "baseline", &services, 2, Duration::ZERO).unwrap();
        assert_eq!(state.current.as_deref(), Some("baseline"));
        assert_eq!(state.previous.as_deref(), Some("next"));
    }
    #[test]
    fn crash_rolls_back_without_losing_previous_state() {
        let (_tmp, store, mut state) = setup();
        let services = Fake {
            fail: true,
            starts: Cell::new(0),
        };
        assert!(activate(&store, &mut state, "next", &services, 2, Duration::ZERO).is_err());
        assert_eq!(state.current.as_deref(), Some("baseline"));
        assert!(state.pending.is_none());
        assert_eq!(
            fs::read_link(store.root.join("current")).unwrap(),
            Path::new("releases/baseline")
        );
        assert!(state.last_result.unwrap().contains("simulated crash"));
    }
}
