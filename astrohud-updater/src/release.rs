use crate::Result;
use ring::{digest, signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

pub const TARGET: &str = "aarch64-unknown-linux-gnu";
pub const MAX_BINARY: u64 = 64 * 1024 * 1024;
pub const MAX_METADATA: u64 = 32 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema: u32,
    pub version: String,
    pub sequence: u64,
    pub expires_unix: u64,
    pub target: String,
    pub frame: Artifact,
    pub provisioner: Artifact,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    // Sign the exact UTF-8 payload, avoiding JSON canonicalization ambiguity.
    pub payload: String,
    pub signature: String,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(value: &str, size: usize) -> Result<Vec<u8>> {
    if value.len() != size * 2 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("expected {} hexadecimal bytes", size).into());
    }
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(Into::into))
        .collect()
}

pub fn https(value: &str) -> Result<Url> {
    let url = Url::parse(value)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("release URLs must use HTTPS without credentials or fragments".into());
    }
    Ok(url)
}

pub fn now() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

pub fn verify(bytes: &[u8], key: &str, now: u64) -> Result<Release> {
    if bytes.len() as u64 > MAX_METADATA {
        return Err("release metadata too large".into());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, unhex(key, 32)?)
        .verify(
            envelope.payload.as_bytes(),
            &unhex(&envelope.signature, 64)?,
        )
        .map_err(|_| "release signature verification failed")?;
    let release: Release = serde_json::from_str(&envelope.payload)?;
    if release.schema != 1 || release.target != TARGET || release.sequence == 0 {
        return Err("unsupported release schema, target, or sequence".into());
    }
    let version = Version::parse(&release.version)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err("the stable channel accepts plain release versions only".into());
    }
    if now < 1_700_000_000 || release.expires_unix <= now {
        return Err("release metadata expired or system clock is not synchronized".into());
    }
    for artifact in [&release.frame, &release.provisioner] {
        https(&artifact.url)?;
        unhex(&artifact.sha256, 32)?;
        if artifact.bytes < 64 || artifact.bytes > MAX_BINARY {
            return Err("release binary size is outside the allowed range".into());
        }
    }
    Ok(release)
}

pub fn agent() -> ureq::Agent {
    // GitHub release assets redirect to their CDN. Refuse HTTP at every hop;
    // this separate client never carries the frame's private credentials.
    ureq::AgentBuilder::new()
        .https_only(true)
        .redirects(5)
        .timeout(Duration::from_secs(120))
        .build()
}

pub fn fetch(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    https(url)?;
    let response = agent.get(url).call()?;
    if response.status() != 200 {
        return Err("release server did not return HTTP 200".into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("release response exceeds size limit".into());
    }
    Ok(bytes)
}

pub fn verify_binary(bytes: &[u8], artifact: &Artifact) -> Result<()> {
    if bytes.len() as u64 != artifact.bytes
        || hex(digest::digest(&digest::SHA256, bytes).as_ref())
            != artifact.sha256.to_ascii_lowercase()
    {
        return Err("binary length or SHA-256 mismatch".into());
    }
    // ELF64, little endian, AArch64. Do not execute unvalidated downloads.
    if bytes.len() < 64 || &bytes[..6] != b"\x7fELF\x02\x01" || bytes[18..20] != [183, 0] {
        return Err("release is not an ARM64 Linux ELF executable".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;
    pub fn fixture() -> (Release, Vec<u8>, String, Vec<u8>) {
        let mut binary = vec![0; 64];
        binary[..6].copy_from_slice(b"\x7fELF\x02\x01");
        binary[18] = 183;
        let artifact = Artifact {
            url: "https://updates.example/frame".into(),
            bytes: 64,
            sha256: hex(digest::digest(&digest::SHA256, &binary).as_ref()),
        };
        let release = Release {
            schema: 1,
            version: "0.6.0".into(),
            sequence: 6,
            expires_unix: 2_000_000_000,
            target: TARGET.into(),
            frame: artifact.clone(),
            provisioner: artifact,
        };
        let pair = signature::Ed25519KeyPair::from_seed_unchecked(&[42; 32]).unwrap();
        let payload = serde_json::to_string(&release).unwrap();
        let envelope = Envelope {
            signature: hex(pair.sign(payload.as_bytes()).as_ref()),
            payload,
        };
        (
            release,
            serde_json::to_vec(&envelope).unwrap(),
            hex(pair.public_key().as_ref()),
            binary,
        )
    }
    #[test]
    fn signed_metadata_and_tampering() {
        let (_, bytes, key, _) = fixture();
        assert!(verify(&bytes, &key, 1_800_000_000).is_ok());
        let mut envelope: Envelope = serde_json::from_slice(&bytes).unwrap();
        envelope.payload = envelope.payload.replace("0.6.0", "9.9.9");
        assert!(verify(&serde_json::to_vec(&envelope).unwrap(), &key, 1_800_000_000).is_err());
        assert!(verify(&bytes, &"00".repeat(32), 1_800_000_000).is_err());
        assert!(verify(&bytes, &key, 2_000_000_000).is_err());
        assert!(verify(&bytes, &key, 0).is_err());
    }
    #[test]
    fn binary_integrity_and_target() {
        let (release, _, _, mut binary) = fixture();
        verify_binary(&binary, &release.frame).unwrap();
        binary[30] = 1;
        assert!(verify_binary(&binary, &release.frame).is_err());
        binary[18] = 62;
        let artifact = Artifact {
            sha256: hex(digest::digest(&digest::SHA256, &binary).as_ref()),
            ..release.frame
        };
        assert!(verify_binary(&binary, &artifact).is_err());
    }
    #[test]
    fn unsafe_urls_and_hex_are_rejected() {
        for url in [
            "http://example/a",
            "https://user:secret@example/a",
            "file:///tmp/a",
            "https://example/a#x",
        ] {
            assert!(https(url).is_err());
        }
        assert!(unhex("é", 1).is_err());
        assert!(unhex("ab", 32).is_err());
    }
}
