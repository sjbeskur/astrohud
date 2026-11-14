//! Headless control plane: only authenticated owner requests can install.
use crate::{
    Result, release,
    store::{self, Check, Store},
};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path, process::Command, thread, time::Duration};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RemoteCommand {
    id: String,
    action: String,
    release_id: Option<String>,
    state: String,
    expires_unix: u64,
    message: Option<String>,
}
#[derive(Deserialize)]
struct Response {
    command: Option<RemoteCommand>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Journal {
    id: String,
    state: String,
    message: String,
}
#[derive(Serialize)]
struct Report {
    updater_version: &'static str,
    installed_release: Option<String>,
    previous_release: Option<String>,
    available_release: Option<String>,
    available_version: Option<String>,
    checked_unix: Option<u64>,
    metadata_expires_unix: Option<u64>,
    last_result: Option<String>,
    request_id: Option<String>,
    request_state: Option<String>,
    message: Option<String>,
}

fn report(store: &Store, journal: Option<&Journal>) -> Result<Report> {
    let state = store.state()?;
    let check: Option<Check> = match fs::read(store.root.join("check.json")) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    Ok(Report {
        updater_version: env!("CARGO_PKG_VERSION"),
        installed_release: state.current,
        previous_release: state.previous,
        available_release: check
            .as_ref()
            .map(|c| format!("{}-{}", c.sequence, c.version)),
        available_version: check.as_ref().map(|c| c.version.clone()),
        checked_unix: check.as_ref().map(|c| c.checked_unix),
        metadata_expires_unix: check.as_ref().map(|c| c.expires_unix),
        last_result: state.last_result.map(|s| s.chars().take(500).collect()),
        request_id: journal.map(|j| j.id.clone()),
        request_state: journal.map(|j| j.state.clone()),
        message: journal.map(|j| j.message.clone()),
    })
}

fn exchange(
    agent: &ureq::Agent,
    endpoint: &str,
    credential: &str,
    body: &Report,
) -> Result<Response> {
    // Never follow redirects with device credentials, even within HTTPS.
    let response = agent
        .post(endpoint)
        .set("Authorization", &format!("Bearer {credential}"))
        .set("Content-Type", "application/json")
        .send_string(&serde_json::to_string(body)?)
        .map_err(|_| "device update control connection unavailable")?;
    if response.status() != 200 {
        return Err("device update control did not return HTTP 200".into());
    }
    let mut bytes = Vec::new();
    response.into_reader().take(16385).read_to_end(&mut bytes)?;
    if bytes.len() > 16384 {
        return Err("device update control response is too large".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn save_journal(store: &Store, journal: &Journal) -> Result<()> {
    store::atomic_write(
        &store.root.join("remote-request.json"),
        &serde_json::to_vec(journal)?,
        0o600,
    )
}
fn validate(command: &RemoteCommand, now: u64) -> Result<()> {
    if command.id.len() != 36
        || !command.id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return Err("invalid update request identifier".into());
    }
    if command.expires_unix <= now || !["pending", "running"].contains(&command.state.as_str()) {
        return Err("expired or completed update request".into());
    }
    match command.action.as_str() {
        "check" if command.release_id.is_none() => (),
        "update" | "rollback" => {
            let id = command
                .release_id
                .as_ref()
                .ok_or("update request has no release identifier")?;
            if id.is_empty()
                || id.len() > 100
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            {
                return Err("invalid requested release".into());
            }
        }
        _ => return Err("unknown update request action".into()),
    }
    Ok(())
}

fn execute(
    store: &Store,
    journal: &mut Option<Journal>,
    command: &RemoteCommand,
    acknowledge: impl FnOnce(&Journal) -> Result<bool>,
    run: impl FnOnce() -> bool,
) -> Result<()> {
    validate(command, release::now()?)?;
    if journal.as_ref().is_some_and(|j| j.id == command.id) {
        return Ok(());
    }
    let mut next = Journal {
        id: command.id.clone(),
        state: "running".into(),
        message: format!("{} request accepted by the frame", command.action),
    };
    save_journal(store, &next)?;
    *journal = Some(next.clone());
    // A lost acknowledgement must become a reported failure, never a stranded
    // running request and never an unconfirmed install.
    if acknowledge(&next).unwrap_or(false) && command.expires_unix > release::now()? {
        let success = run();
        next.state = if success { "succeeded" } else { "failed" }.into();
        next.message = if success {
            match command.action.as_str() {
                "check" => "Update check complete.",
                "rollback" => "Previous application release restored.",
                _ => "Application update complete.",
            }
        } else {
            "The frame could not complete this request. Its previous installation was kept or recovery was attempted; check status before retrying."
        }.into();
    } else {
        next.state = "failed".into();
        next.message = "The frame could not confirm this request. No installation was started; try again when connected.".into();
    }
    save_journal(store, &next)?;
    *journal = Some(next);
    Ok(())
}

pub fn serve(store: &Store, config: &Path) -> Result<()> {
    // Separate from the install lock: restarting the agent cannot create a
    // second executor, but child installer commands can acquire their lock.
    let singleton = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(store.root.join("agent.lock"))?;
    singleton
        .try_lock()
        .map_err(|_| "another update agent is running")?;
    let server =
        std::env::var("ASTROHUD_SERVER_URL").unwrap_or_else(|_| "https://app.astrohud.com".into());
    let base = release::https(&server)?;
    let endpoint = base.join("/api/beta/device/updates")?;
    let agent = ureq::AgentBuilder::new()
        .https_only(true)
        .redirects(0)
        .timeout(Duration::from_secs(20))
        .build();
    let mut journal: Option<Journal> = match fs::read(store.root.join("remote-request.json")) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if let Some(j) = journal.as_mut()
        && j.state == "running"
    {
        // Never replay an install after an agent crash. A reboot recovers
        // the install journal; during a service restart, recover it now.
        let _ = Command::new("/usr/local/bin/memory-lane")
            .arg("recover")
            .status();
        j.state = "failed".into();
        j.message="The update was interrupted. The previous installation was kept or recovered; check the frame before retrying.".into();
        save_journal(store, j)?;
    }
    loop {
        let attempt = (|| -> Result<()> {
            // Re-read after every reset: never retain a revoked device secret.
            let credential = fs::read_to_string("/var/lib/astrohud/device-credential")?;
            let response = exchange(
                &agent,
                endpoint.as_str(),
                credential.trim(),
                &report(store, journal.as_ref())?,
            )?;
            let Some(command) = response.command else {
                return Ok(());
            };
            execute(
                store,
                &mut journal,
                &command,
                |next| {
                    let accepted = exchange(
                        &agent,
                        endpoint.as_str(),
                        credential.trim(),
                        &report(store, Some(next))?,
                    )?;
                    Ok(accepted
                        .command
                        .as_ref()
                        .is_some_and(|c| c.id == command.id && c.state == "running"))
                },
                || {
                    let mut child = Command::new("/usr/local/bin/memory-lane");
                    child.arg("--config").arg(config).arg(&command.action);
                    if let Some(expected) = &command.release_id {
                        child.arg("--expect").arg(expected);
                    }
                    child.status().is_ok_and(|s| s.success())
                },
            )?;
            // Report completion immediately; lost responses are retried by the
            // next heartbeat without running the command again.
            exchange(
                &agent,
                endpoint.as_str(),
                credential.trim(),
                &report(store, journal.as_ref())?,
            )?;
            Ok(())
        })();
        if let Err(error) = attempt {
            eprintln!("update agent: {error}");
        }
        thread::sleep(Duration::from_secs(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command() -> RemoteCommand {
        RemoteCommand {
            id: "12345678-1234-1234-1234-123456789abc".into(),
            action: "update".into(),
            release_id: Some("6-0.6.0".into()),
            state: "pending".into(),
            expires_unix: 2_000_000_000,
            message: None,
        }
    }
    #[test]
    fn requests_are_expiring_bounded_actions_with_exact_releases() {
        let mut c = command();
        validate(&c, 1_800_000_000).unwrap();
        assert!(validate(&c, 2_000_000_000).is_err());
        c.action = "sh".into();
        assert!(validate(&c, 1_800_000_000).is_err());
        c.action = "update".into();
        c.release_id = None;
        assert!(validate(&c, 1_800_000_000).is_err());
        c.action = "check".into();
        validate(&c, 1_800_000_000).unwrap();
        c.id = "../../unsafe".into();
        assert!(validate(&c, 1_800_000_000).is_err());
    }
    #[test]
    fn lost_acknowledgement_never_starts_install_and_replay_is_harmless() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store {
            root: dir.path().into(),
        };
        let mut journal = None;
        let calls = std::cell::Cell::new(0);
        execute(
            &store,
            &mut journal,
            &command(),
            |_| Err("lost response".into()),
            || {
                calls.set(calls.get() + 1);
                true
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 0);
        assert_eq!(journal.as_ref().unwrap().state, "failed");
        // Simulate process restart and the same server command being delivered.
        let mut restored = Some(
            serde_json::from_slice(&fs::read(store.root.join("remote-request.json")).unwrap())
                .unwrap(),
        );
        execute(
            &store,
            &mut restored,
            &command(),
            |_| Ok(true),
            || {
                calls.set(calls.get() + 1);
                true
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 0);
    }
    #[test]
    fn successful_request_runs_once_even_when_completion_response_is_lost() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store {
            root: dir.path().into(),
        };
        let mut journal = None;
        let calls = std::cell::Cell::new(0);
        execute(
            &store,
            &mut journal,
            &command(),
            |_| Ok(true),
            || {
                calls.set(calls.get() + 1);
                true
            },
        )
        .unwrap();
        execute(
            &store,
            &mut journal,
            &command(),
            |_| Ok(true),
            || {
                calls.set(calls.get() + 1);
                true
            },
        )
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(journal.unwrap().state, "succeeded");
    }
}
