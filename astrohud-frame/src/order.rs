//! ID-based playback selection, independent of network sync and image rendering.
use crate::BoxError;
use rand::{Rng, seq::SliceRandom};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    ScenicRoute,
    #[default]
    FreshPrints,
    Serendipity,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybackOrder {
    version: u8,
    frame_id: String,
    mode: PlaybackMode,
    ids: Vec<String>,
    remaining: Vec<String>,
    seen: HashSet<String>,
    current: Option<String>,
    #[serde(default)]
    owner_revision: u64,
    #[serde(skip)]
    pending: Option<String>,
}
impl Default for PlaybackOrder {
    fn default() -> Self {
        Self {
            version: 1,
            frame_id: String::new(),
            mode: PlaybackMode::default(),
            ids: vec![],
            remaining: vec![],
            seen: HashSet::new(),
            current: None,
            owner_revision: 0,
            pending: None,
        }
    }
}
impl PlaybackOrder {
    pub fn reconcile(&mut self, frame: &str, mode: PlaybackMode, ids: Vec<String>) {
        if self.frame_id != frame {
            *self = Self {
                frame_id: frame.into(),
                mode,
                ..Self::default()
            };
        } else if self.mode != mode {
            self.mode = mode;
            self.remaining.clear();
            self.seen.clear();
            self.pending = None;
            // A mode change begins a new sequence at the next slide boundary.
            self.current = None;
        }
        let mut unique = HashSet::new();
        self.ids = ids
            .into_iter()
            .filter(|id| unique.insert(id.clone()))
            .collect();
        self.remaining.retain(|id| unique.contains(id));
        self.seen.retain(|id| unique.contains(id));
        if self.current.as_ref().is_some_and(|id| !unique.contains(id)) {
            self.current = None;
        }
        if self.pending.as_ref().is_some_and(|id| !unique.contains(id)) {
            self.pending = None;
        }
        // New uploads join the unfinished round without replaying seen photos.
        let mut queued: HashSet<String> = self.remaining.iter().cloned().collect();
        for id in &self.ids {
            if !self.seen.contains(id) && queued.insert(id.clone()) {
                self.remaining.push(id.clone());
            }
        }
    }
    pub fn peek(&mut self, rng: &mut impl Rng) -> Option<&str> {
        if self.ids.is_empty() {
            return None;
        }
        if self.pending.is_none() {
            self.pending = match self.mode {
                PlaybackMode::FreshPrints => {
                    let index = self
                        .current
                        .as_ref()
                        .and_then(|id| self.ids.iter().position(|p| p == id))
                        .map(|i| (i + 1) % self.ids.len())
                        .unwrap_or(0);
                    Some(self.ids[index].clone())
                }
                PlaybackMode::Serendipity => {
                    Some(self.ids[rng.random_range(0..self.ids.len())].clone())
                }
                PlaybackMode::ScenicRoute => {
                    if self.remaining.is_empty() {
                        self.seen.clear();
                        self.remaining = self.ids.clone();
                    }
                    // Shuffle only when selecting a new candidate. Prefetching
                    // peeks at the same draw until a photo is actually shown.
                    self.remaining.shuffle(rng);
                    let last = self.remaining.len() - 1;
                    if last > 0 && self.remaining.last() == self.current.as_ref() {
                        let other = rng.random_range(0..last);
                        self.remaining.swap(last, other);
                    }
                    self.remaining.last().cloned()
                }
            };
        }
        self.pending.as_deref()
    }
    pub fn owner_revision(&self) -> u64 {
        self.owner_revision
    }
    pub fn consume_owner_revision(&mut self, revision: u64) {
        self.owner_revision = revision;
    }
    pub fn shown(&mut self, id: &str) {
        if !self.ids.iter().any(|p| p == id) {
            return;
        }
        self.current = Some(id.into());
        self.pending = None;
        if self.mode == PlaybackMode::ScenicRoute {
            self.seen.insert(id.into());
            self.remaining.retain(|p| p != id);
        }
    }
    pub fn load(path: &Path) -> Result<Self, BoxError> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("playback progress file is too large".into());
        }
        let state: Self = serde_json::from_slice(&bytes)?;
        if state.version != 1 {
            return Err("unsupported playback progress version".into());
        }
        Ok(state)
    }
    pub fn save(&self, path: &Path) -> Result<(), BoxError> {
        let temp = path.with_extension("new");
        let mut file = File::create(&temp)?;
        file.write_all(&serde_json::to_vec(self)?)?;
        file.sync_all()?;
        fs::rename(temp, path)?;
        if let Some(parent) = path.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{SeedableRng, rngs::StdRng};
    fn ids() -> Vec<String> {
        ["new", "middle", "old"].map(String::from).to_vec()
    }
    #[test]
    fn scenic_rounds_include_every_photo_without_sync_or_prefetch_repeats() {
        let mut rng = StdRng::seed_from_u64(71);
        let mut order = PlaybackOrder::default();
        for _ in 0..5 {
            let mut round = HashSet::new();
            for _ in 0..3 {
                order.reconcile("frame", PlaybackMode::ScenicRoute, ids());
                let id = order.peek(&mut rng).unwrap().to_string();
                assert_eq!(order.peek(&mut rng), Some(id.as_str()));
                assert!(round.insert(id.clone()));
                order.shown(&id);
            }
            assert_eq!(round.len(), 3);
        }
    }
    #[test]
    fn progress_survives_restart_and_membership_changes() {
        let path = std::env::temp_dir().join(format!(
            "memory-lane-order-{}-{}.json",
            std::process::id(),
            rand::random::<u64>()
        ));
        let mut rng = StdRng::seed_from_u64(9);
        let mut order = PlaybackOrder::default();
        order.reconcile("frame", PlaybackMode::ScenicRoute, ids());
        let first = order.peek(&mut rng).unwrap().to_string();
        order.shown(&first);
        order.consume_owner_revision(7);
        order.save(&path).unwrap();
        let mut restored = PlaybackOrder::load(&path).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(restored.owner_revision(), 7);
        restored.reconcile("frame", PlaybackMode::ScenicRoute, ids());
        let second = restored.peek(&mut rng).unwrap().to_string();
        assert_ne!(first, second);
        restored.shown(&second);
        let mut changed = ids();
        changed.push("arriving".into());
        changed.retain(|id| id != &first);
        restored.reconcile("frame", PlaybackMode::ScenicRoute, changed);
        let mut upcoming = HashSet::new();
        for _ in 0..2 {
            let id = restored.peek(&mut rng).unwrap().to_string();
            assert_ne!(id, second);
            upcoming.insert(id.clone());
            restored.shown(&id);
        }
        assert!(upcoming.contains("arriving"));
        assert_eq!(upcoming.len(), 2);
    }
    #[test]
    fn fresh_prints_cycles_newest_first_and_random_can_repeat() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut order = PlaybackOrder::default();
        order.reconcile("frame", PlaybackMode::FreshPrints, ids());
        for expected in ["new", "middle", "old", "new"] {
            assert_eq!(order.peek(&mut rng), Some(expected));
            order.shown(expected);
        }
        order.reconcile("frame", PlaybackMode::Serendipity, ids());
        let mut previous = String::new();
        let mut repeat = false;
        let mut seen = HashSet::new();
        for _ in 0..100 {
            let id = order.peek(&mut rng).unwrap().to_string();
            repeat |= id == previous;
            seen.insert(id.clone());
            order.shown(&id);
            previous = id;
        }
        assert!(repeat);
        assert_eq!(seen.len(), 3);
    }
    #[test]
    fn empty_single_and_other_frames_are_safe() {
        let mut rng = StdRng::seed_from_u64(4);
        let mut order = PlaybackOrder::default();
        order.reconcile("a", PlaybackMode::ScenicRoute, vec![]);
        assert!(order.peek(&mut rng).is_none());
        order.reconcile("a", PlaybackMode::ScenicRoute, vec!["only".into()]);
        for _ in 0..3 {
            assert_eq!(order.peek(&mut rng), Some("only"));
            order.shown("only");
        }
        order.reconcile("b", PlaybackMode::FreshPrints, ids());
        assert_eq!(order.peek(&mut rng), Some("new"));
    }
}
