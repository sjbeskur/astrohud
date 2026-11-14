use crate::{
    Result,
    release::{self, Release},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

pub const FRAME: &str = "astrohud-frame";
pub const PROVISIONER: &str = "astrohud-provisioner";

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub current: Option<String>,
    pub previous: Option<String>,
    pub pending: Option<Pending>,
    pub highest_sequence: u64,
    pub highest_version: Option<String>,
    pub last_result: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub from: String,
    pub to: String,
}

#[derive(Deserialize, Serialize)]
pub struct Check {
    pub checked_unix: u64,
    pub version: String,
    pub sequence: u64,
    pub expires_unix: u64,
}

pub struct Store {
    pub root: PathBuf,
}

pub fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let temp = path.with_extension("new");
    // All writers hold the store lock. Remove only our own abandoned temp file.
    if temp.exists() {
        fs::remove_file(&temp)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    sync_dir(path.parent().ok_or("missing parent directory")?)
}

pub fn atomic_link(path: &Path, target: &Path) -> Result<()> {
    let temp = path.with_extension("link-new");
    if fs::symlink_metadata(&temp).is_ok() {
        fs::remove_file(&temp)?;
    }
    symlink(target, &temp)?;
    fs::rename(&temp, path)?;
    sync_dir(path.parent().ok_or("missing parent directory")?)
}

impl Store {
    pub fn lock(&self) -> Result<File> {
        fs::create_dir_all(self.root.join("releases"))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join("lock"))?;
        file.try_lock()
            .map_err(|_| "another updater command is running")?;
        Ok(file)
    }
    pub fn state(&self) -> Result<State> {
        match fs::read(self.root.join("state.json")) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e.into()),
        }
    }
    pub fn save(&self, state: &State) -> Result<()> {
        atomic_write(
            &self.root.join("state.json"),
            &serde_json::to_vec_pretty(state)?,
            0o644,
        )
    }
    pub fn prune(&self, state: &State) -> Result<()> {
        self.ensure_current(state)?;
        if state.pending.is_some() {
            return Err("cannot prune releases during an interrupted update".into());
        }
        for entry in fs::read_dir(self.root.join("releases"))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("invalid local release name")?;
            if name == "baseline"
                || state.current.as_deref() == Some(name)
                || state.previous.as_deref() == Some(name)
            {
                continue;
            }
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(self.release_dir(name)?)?;
            }
        }
        sync_dir(&self.root.join("releases"))
    }
    pub fn release_dir(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty()
            || id.len() > 100
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            || id == "."
            || id == ".."
        {
            return Err("invalid local release identifier".into());
        }
        Ok(self.root.join("releases").join(id))
    }
    pub fn switch(&self, id: &str) -> Result<()> {
        let dir = self.release_dir(id)?;
        for name in [FRAME, PROVISIONER] {
            if !dir.join(name).is_file() {
                return Err(format!("missing local binary: {name}").into());
            }
        }
        atomic_link(&self.root.join("current"), &Path::new("releases").join(id))
    }
    pub fn ensure_current(&self, state: &State) -> Result<()> {
        let id = state
            .current
            .as_ref()
            .ok_or("run `sudo memory-lane init` first")?;
        if fs::read_link(self.root.join("current"))? != Path::new("releases").join(id) {
            return Err("release pointer differs from recorded state; run recover".into());
        }
        Ok(())
    }
    pub fn stage(&self, release: &Release, frame: &[u8], provisioner: &[u8]) -> Result<String> {
        release::verify_binary(frame, &release.frame)?;
        release::verify_binary(provisioner, &release.provisioner)?;
        let id = format!("{}-{}", release.sequence, release.version);
        let dir = self.release_dir(&id)?;
        if dir.exists() {
            // Reusing a prior verified release is safe only with identical bytes.
            for (name, artifact) in [(FRAME, &release.frame), (PROVISIONER, &release.provisioner)] {
                release::verify_binary(&fs::read(dir.join(name))?, artifact)?;
            }
            return Ok(id);
        }
        let staging = self.root.join("staging");
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir(&staging)?;
        atomic_write(&staging.join(FRAME), frame, 0o755)?;
        atomic_write(&staging.join(PROVISIONER), provisioner, 0o755)?;
        atomic_write(
            &staging.join("release.json"),
            &serde_json::to_vec_pretty(release)?,
            0o644,
        )?;
        sync_dir(&staging)?;
        fs::rename(&staging, &dir)?;
        sync_dir(&self.root.join("releases"))?;
        Ok(id)
    }
    pub fn recover(&self, state: &mut State) -> Result<bool> {
        let Some(pending) = state.pending.clone() else {
            return Ok(false);
        };
        self.switch(&pending.from)?;
        state.current = Some(pending.from);
        state.pending = None;
        state.last_result = Some(format!("Recovered interrupted switch to {}", pending.to));
        self.save(state)?;
        Ok(true)
    }
    pub fn init(&self, frame: &Path, provisioner: &Path) -> Result<()> {
        let mut state = self.state()?;
        if state.pending.is_some() {
            return Err("recover the interrupted update before initializing".into());
        }
        if state.current.is_some() {
            self.ensure_current(&state)?;
        } else {
            let baseline = self.release_dir("baseline")?;
            fs::create_dir_all(&baseline)?;
            for (name, source) in [(FRAME, frame), (PROVISIONER, provisioner)] {
                // Bootstrap is reentrant across power loss, including a partial
                // migration of the two original binary paths to symlinks.
                let destination = baseline.join(name);
                if !destination.exists() {
                    atomic_write(&destination, &fs::read(source)?, 0o755)?;
                }
            }
            sync_dir(&baseline)?;
            sync_dir(&self.root.join("releases"))?;
            self.switch("baseline")?;
            state.current = Some("baseline".into());
            state.last_result = Some("Adopted existing application binaries".into());
            self.save(&state)?;
        }
        for (name, path) in [(FRAME, frame), (PROVISIONER, provisioner)] {
            atomic_link(path, &self.root.join("current").join(name))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_release_paths_are_rejected() {
        let store = Store {
            root: "/unused".into(),
        };
        for id in ["..", ".", "../etc", "/tmp", "", "a/b"] {
            assert!(store.release_dir(id).is_err());
        }
    }
    #[test]
    fn interrupted_switch_restores_both_binaries_and_state() {
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
        let candidate = store.release_dir("candidate").unwrap();
        fs::create_dir(&candidate).unwrap();
        fs::write(candidate.join(FRAME), b"new frame").unwrap();
        fs::write(candidate.join(PROVISIONER), b"new provisioner").unwrap();
        let mut state = store.state().unwrap();
        state.pending = Some(Pending {
            from: "baseline".into(),
            to: "candidate".into(),
        });
        store.save(&state).unwrap();
        store.switch("candidate").unwrap();
        assert_eq!(fs::read(&frame).unwrap(), b"new frame");
        let mut state = store.state().unwrap();
        assert!(store.recover(&mut state).unwrap());
        assert_eq!(fs::read(&frame).unwrap(), b"old frame");
        assert_eq!(fs::read(&provisioner).unwrap(), b"old provisioner");
        assert!(!store.recover(&mut state).unwrap());
        store.init(&frame, &provisioner).unwrap();
        assert_eq!(fs::read(&frame).unwrap(), b"old frame");
    }
    #[test]
    fn exclusive_lock_survives_abandoned_lock_file() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store {
            root: tmp.path().into(),
        };
        let lock = store.lock().unwrap();
        assert!(store.lock().is_err());
        drop(lock);
        assert!(store.lock().is_ok());
    }
}
