use crate::BoxError;
use crate::cache::CacheStore;
use crate::model::{FrameManifest, Photo, Playlist};
use std::io::Read;
use std::time::Duration;
use url::Url;

const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

pub struct ServerClient {
    base_url: Url,
    frame_id: String,
    device_credential: Option<String>,
    agent: ureq::Agent,
}

pub struct SyncResult {
    pub playlist: Playlist,
    pub warnings: Vec<String>,
}

impl ServerClient {
    pub fn new(
        base_url: &str,
        frame_id: impl Into<String>,
        device_credential: Option<String>,
    ) -> Result<Self, BoxError> {
        let base_url = Url::parse(base_url)?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err("server URL must use HTTP or HTTPS".into());
        }
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout_read(Duration::from_secs(30))
            .build();
        Ok(Self {
            base_url,
            frame_id: frame_id.into(),
            device_credential,
            agent,
        })
    }

    pub fn fetch_manifest(&self) -> Result<FrameManifest, BoxError> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| "server URL cannot be a base URL")?;
            segments.clear();
            if self.device_credential.is_some() {
                segments.extend(["api", "beta", "device", "manifest"]);
            } else {
                segments.extend(["api", "frames", &self.frame_id, "manifest"]);
            }
        }

        let response = self.authorized_get(url.as_str()).call()?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("frame manifest exceeds 2 MiB".into());
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn download(&self, photo: &Photo, cache: &CacheStore) -> Result<(), BoxError> {
        let url = self.base_url.join(&photo.url)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("photo URL must use HTTP or HTTPS".into());
        }
        let response = self.authorized_get(url.as_str()).call()?;
        cache.install(photo, response.into_reader())?;
        Ok(())
    }

    fn authorized_get(&self, url: &str) -> ureq::Request {
        let request = self.agent.get(url);
        match &self.device_credential {
            Some(credential) => request.set("Authorization", &format!("Bearer {credential}")),
            None => request,
        }
    }
}

pub fn sync_once(client: &ServerClient, cache: &CacheStore) -> Result<SyncResult, BoxError> {
    let manifest = client.fetch_manifest()?;
    let mut warnings = Vec::new();
    let previous = cache.load_manifest()?;
    let manifest_is_unchanged = previous
        .as_ref()
        .is_some_and(|previous| previous.photos == manifest.photos);
    let oldest_cached_index = manifest
        .photos
        .iter()
        .enumerate()
        .filter(|(_, photo)| cache.contains(photo))
        .map(|(index, _)| index)
        .max();

    // The manifest is newest-first. Once newer cached images fill the budget,
    // older missing images are deliberately not downloaded.
    for (index, photo) in manifest.photos.iter().enumerate() {
        if !photo.is_supported() || cache.contains(photo) {
            continue;
        }

        // An unchanged manifest may contain older photos intentionally evicted
        // by the byte cap. Do retry gaps among newer cached photos (a prior
        // download may have failed), but do not fetch evicted history forever.
        if manifest_is_unchanged && oldest_cached_index.is_some_and(|oldest| index > oldest) {
            continue;
        }

        if let Err(error) = client.download(photo, cache) {
            warnings.push(format!("could not cache photo {}: {error}", photo.id));
            continue;
        }
        // Reconcile after each download so a new image can replace the oldest
        // cached image without allowing the cache to grow without bound.
        cache.reconcile(&manifest)?;
        if !cache.contains(photo) {
            break;
        }
    }

    let playlist = cache.reconcile(&manifest)?;
    cache.save_manifest(&manifest)?;
    Ok(SyncResult { playlist, warnings })
}
