mod install;
mod release;
mod remote;
mod store;

use install::Services;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use store::{Check, Store};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const HELP: &str = "Memory Lane application updater\n\nUsage: memory-lane [--config FILE] <COMMAND>\n\n  status     Show installed release, previous release, and last automatic check\n  check      Verify stable-channel metadata and record available version\n  update     Download, verify, install, and check both applications\n  rollback   Switch back to the previous working application release\n  init       Adopt the existing binaries (one-time operator bootstrap)\n  recover    Restore an interrupted update and restart applications\n  recover-boot  Restore an interrupted update before applications start\n\nUse sudo for commands other than status. Updates preserve identity, Wi-Fi,\npairing, and photos. OS and updater upgrades are separate.\n";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    manifest_url: String,
    public_key: String,
}
impl Config {
    fn load(path: &Path) -> Result<Self> {
        let config: Self = serde_json::from_slice(&fs::read(path).map_err(|e| {
            format!(
                "{}: {e}; install the release URL and trusted public key first",
                path.display()
            )
        })?)?;
        release::https(&config.manifest_url)?;
        release::unhex(&config.public_key, 32)?;
        Ok(config)
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("memory-lane: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut config_path = PathBuf::from("/etc/memory-lane-updater.json");
    if args.first().map(String::as_str) == Some("--config") {
        if args.len() < 3 {
            return Err("--config needs a file and command".into());
        }
        config_path = args[1].clone().into();
        args.drain(..2);
    }
    if args.is_empty() || args == ["--help"] || args == ["help"] || args == ["-h"] {
        print!("{HELP}");
        return Ok(());
    }
    if args == ["--version"] {
        println!("memory-lane {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let expected = if args.len() == 3
        && args[1] == "--expect"
        && ["update", "rollback"].contains(&args[0].as_str())
    {
        let value = args[2].clone();
        args.truncate(1);
        Some(value)
    } else {
        None
    };
    if args.len() != 1 {
        return Err("expected one command; see --help".into());
    }
    let command = args[0].as_str();
    if ![
        "status",
        "check",
        "update",
        "rollback",
        "init",
        "recover",
        "recover-boot",
        "agent",
    ]
    .contains(&command)
    {
        return Err(format!("unknown command {command}; see --help").into());
    }
    let store = Store {
        root: "/opt/memory-lane".into(),
    };
    if command == "status" {
        return status(&store);
    }
    // Never honor an unprivileged PATH or alternate root in the privileged CLI.
    if std::os::unix::fs::MetadataExt::uid(&fs::metadata("/proc/self")?) != 0 {
        return Err("this command requires sudo".into());
    }
    if command == "agent" {
        return remote::serve(&store, &config_path);
    }
    if ["init", "update", "rollback"].contains(&command) {
        let active = std::process::Command::new("/usr/bin/systemctl")
            .args(["is-active", "--quiet", "memory-lane-update-recover.service"])
            .status()?;
        if !active.success() {
            return Err("start memory-lane-update-recover.service before managing releases (see bootstrap instructions)".into());
        }
    }
    let _lock = store.lock()?;
    let mut state = store.state()?;
    match command {
        "init" => {
            store.init(
                Path::new("/usr/local/bin/astrohud-frame"),
                Path::new("/usr/local/sbin/astrohud-provisioner"),
            )?;
            println!("Existing binaries adopted; application services were not restarted.");
        }
        "recover-boot" => {
            if store.recover(&mut state)? {
                println!("Interrupted update restored before application startup.");
            }
        }
        "recover" => {
            if state.pending.is_some() {
                install::Systemd.stop()?;
                store.recover(&mut state)?;
                install::Systemd.start()?;
                std::thread::sleep(Duration::from_secs(1));
                install::Systemd
                    .healthy(&store.release_dir(
                        state.current.as_deref().ok_or("missing restored release")?,
                    )?)?;
                println!("Interrupted update restored; application services restarted.");
            } else {
                println!("No interrupted update.");
            }
        }
        "rollback" => {
            install::safe_to_update()?;
            let previous = state
                .previous
                .clone()
                .ok_or("no previous release is available")?;
            if expected.as_ref().is_some_and(|id| id != &previous) {
                return Err(
                    "the previous release changed after the owner confirmed rollback".into(),
                );
            }
            install::activate(
                &store,
                &mut state,
                &previous,
                &install::Systemd,
                15,
                Duration::from_secs(1),
            )?;
            println!("Restored {previous}.");
        }
        "check" | "update" => {
            if command == "update" && state.pending.is_some() {
                return Err("interrupted update pending; run recover first".into());
            }
            let config = Config::load(&config_path)?;
            let agent = release::agent();
            let bytes = release::fetch(&agent, &config.manifest_url, release::MAX_METADATA)?;
            let release = release::verify(&bytes, &config.public_key, release::now()?)?;
            validate_sequence(&release, &state)?;
            // Persist the highest VERIFIED sequence on checks too. Replaying an
            // older signed channel cannot silently downgrade a later update.
            state.highest_sequence = release.sequence;
            state.highest_version = Some(release.version.clone());
            store.save(&state)?;
            store::atomic_write(
                &store.root.join("check.json"),
                &serde_json::to_vec_pretty(&Check {
                    checked_unix: release::now()?,
                    version: release.version.clone(),
                    sequence: release.sequence,
                    expires_unix: release.expires_unix,
                })?,
                0o644,
            )?;
            let id = format!("{}-{}", release.sequence, release.version);
            if expected.as_ref().is_some_and(|expected| expected != &id) {
                return Err("the stable release changed after the owner confirmed installation; check again".into());
            }
            if state.current.as_deref() == Some(&id) {
                println!("Already up to date: {}.", release.version);
                return Ok(());
            }
            if command == "check" {
                println!(
                    "Stable release available: {}. Run `sudo memory-lane update` to install.",
                    release.version
                );
                return Ok(());
            }
            store.ensure_current(&state)?;
            if state.pending.is_some() {
                return Err("interrupted update pending; run recover first".into());
            }
            if cfg!(not(target_arch = "aarch64")) {
                return Err(
                    "installation is supported only on ARM64 Pi Zero 2 W appliances".into(),
                );
            }
            install::safe_to_update()?;
            store.prune(&state)?;
            println!("Downloading and verifying Memory Lane {}…", release.version);
            let frame = release::fetch(&agent, &release.frame.url, release.frame.bytes)?;
            let provisioner =
                release::fetch(&agent, &release.provisioner.url, release.provisioner.bytes)?;
            let id = store.stage(&release, &frame, &provisioner)?;
            // Recheck expiry and appliance state after potentially slow downloads.
            release::verify(&bytes, &config.public_key, release::now()?)?;
            install::safe_to_update()?;
            println!(
                "Installing {}; checking both services for 15 seconds…",
                release.version
            );
            install::activate(
                &store,
                &mut state,
                &id,
                &install::Systemd,
                15,
                Duration::from_secs(1),
            )?;
            if let Err(error) = store.prune(&state) {
                eprintln!("Installed successfully; older-release cleanup needs attention: {error}");
            }
            println!(
                "Installed {}. Previous release retained for rollback.",
                release.version
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn validate_sequence(release: &release::Release, state: &store::State) -> Result<()> {
    if release.sequence < state.highest_sequence {
        return Err("refusing older signed release metadata (sequence replay)".into());
    }
    if let Some(highest) = &state.highest_version {
        let new_version = semver::Version::parse(&release.version)?;
        let highest_version = semver::Version::parse(highest)?;
        if new_version < highest_version
            || (release.sequence == state.highest_sequence && new_version != highest_version)
        {
            return Err(
                "release version regressed or sequence was reused for a different version".into(),
            );
        }
    }
    Ok(())
}

fn status(store: &Store) -> Result<()> {
    let state = store.state()?;
    println!(
        "Installed: {}",
        state
            .current
            .as_deref()
            .unwrap_or("unmanaged (run sudo memory-lane init)")
    );
    println!("Previous:  {}", state.previous.as_deref().unwrap_or("none"));
    if let Some(pending) = &state.pending {
        println!(
            "Interrupted update: {} → {}; run sudo memory-lane recover",
            pending.from, pending.to
        );
    }
    if let Some(result) = &state.last_result {
        println!("Last result: {result}");
    }
    match fs::read(store.root.join("check.json")) {
        Ok(bytes) => {
            let check: Check = serde_json::from_slice(&bytes)?;
            let now = release::now()?;
            let age_minutes = now.saturating_sub(check.checked_unix) / 60;
            println!(
                "Stable: {} (checked {age_minutes} minutes ago)",
                check.version
            );
            if now >= check.expires_unix {
                println!("Check is stale; run sudo memory-lane check.");
            } else if state.current.as_deref()
                == Some(&format!("{}-{}", check.sequence, check.version))
            {
                println!("Up to date.");
            } else {
                println!("Update available; run sudo memory-lane update.");
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("No verified release check yet.")
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_replayed_channel_after_a_newer_check() {
        let release = release::Release {
            schema: 1,
            version: "0.6.0".into(),
            sequence: 6,
            expires_unix: 2_000_000_000,
            target: release::TARGET.into(),
            frame: release::Artifact {
                url: "https://example/a".into(),
                bytes: 64,
                sha256: "00".repeat(32),
            },
            provisioner: release::Artifact {
                url: "https://example/b".into(),
                bytes: 64,
                sha256: "00".repeat(32),
            },
        };
        let state = store::State {
            highest_sequence: 7,
            ..Default::default()
        };
        assert!(validate_sequence(&release, &state).is_err());
        assert!(validate_sequence(&release, &store::State::default()).is_ok());
    }
}
