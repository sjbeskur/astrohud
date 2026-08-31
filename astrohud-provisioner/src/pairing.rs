use crate::identity::DeviceIdentity;
use crate::setup_display;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::thread;
use std::time::Duration;
use url::Url;

const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_secs(3);

pub fn ensure_claimed(
    identity: &DeviceIdentity,
    server_url: &str,
    display_path: &Path,
) -> Result<(), String> {
    let client = PairingClient::new(server_url)?;
    if client.is_claimed(&identity.device_credential)? {
        return setup_display::remove(display_path).map_err(|error| error.to_string());
    }

    loop {
        let mut enrollment = client.register(identity)?;
        loop {
            match enrollment {
                Enrollment::Claimed => {
                    setup_display::remove(display_path).map_err(|error| error.to_string())?;
                    return Ok(());
                }
                Enrollment::Pending {
                    ref enrollment_id,
                    ref claim_code,
                } => {
                    let claim_url = client.owner_claim_url(claim_code)?;
                    setup_display::write_claim(identity, claim_code, &claim_url, display_path)?;
                    thread::sleep(POLL_INTERVAL);
                    match client.status(enrollment_id, &identity.device_credential)? {
                        Some(next) => enrollment = next,
                        None => break,
                    }
                }
            }
        }
    }
}

struct PairingClient {
    base_url: Url,
    agent: ureq::Agent,
}

impl PairingClient {
    fn new(server_url: &str) -> Result<Self, String> {
        let base_url = Url::parse(server_url).map_err(|error| error.to_string())?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err("server URL must use HTTP or HTTPS".to_owned());
        }
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(15))
            .build();
        Ok(Self { base_url, agent })
    }

    fn is_claimed(&self, credential: &str) -> Result<bool, String> {
        let url = self.endpoint(&["api", "beta", "device", "manifest"])?;
        match self
            .agent
            .get(url.as_str())
            .set("Authorization", &format!("Bearer {credential}"))
            .call()
        {
            Ok(_) => Ok(true),
            Err(ureq::Error::Status(401, _)) => Ok(false),
            Err(error) => Err(format!("could not check device claim: {error}")),
        }
    }

    fn register(&self, identity: &DeviceIdentity) -> Result<Enrollment, String> {
        let url = self.endpoint(&["api", "beta", "devices", "enrollments"])?;
        let body = serde_json::to_string(&RegistrationRequest {
            device_id: &identity.device_id,
            device_code: &identity.device_code,
            credential: &identity.device_credential,
        })
        .map_err(|error| error.to_string())?;
        let response = self
            .agent
            .post(url.as_str())
            .set("Content-Type", "application/json")
            .send_string(&body)
            .map_err(|error| format!("could not enroll device: {error}"))?;
        decode_enrollment(response)
    }

    fn status(&self, enrollment_id: &str, credential: &str) -> Result<Option<Enrollment>, String> {
        let url = self.endpoint(&["api", "beta", "devices", "enrollments", enrollment_id])?;
        match self
            .agent
            .get(url.as_str())
            .set("Authorization", &format!("Bearer {credential}"))
            .call()
        {
            Ok(response) => decode_enrollment(response).map(Some),
            Err(ureq::Error::Status(409, _)) => Ok(None),
            Err(error) => Err(format!("could not check device enrollment: {error}")),
        }
    }

    fn endpoint(&self, segments: &[&str]) -> Result<Url, String> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| "server URL cannot be a base URL".to_owned())?
            .clear()
            .extend(segments);
        Ok(url)
    }

    fn owner_claim_url(&self, claim_code: &str) -> Result<String, String> {
        let mut url = self.endpoint(&["owner.html"])?;
        url.query_pairs_mut().append_pair("claim_code", claim_code);
        Ok(url.into())
    }
}

#[derive(Serialize)]
struct RegistrationRequest<'a> {
    device_id: &'a str,
    device_code: &'a str,
    credential: &'a str,
}

#[derive(Deserialize)]
struct RegistrationResponse {
    enrollment_id: String,
    status: String,
    claim_code: Option<String>,
}

enum Enrollment {
    Pending {
        enrollment_id: String,
        claim_code: String,
    },
    Claimed,
}

fn decode_enrollment(response: ureq::Response) -> Result<Enrollment, String> {
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return Err("device enrollment response is too large".to_owned());
    }
    let response: RegistrationResponse =
        serde_json::from_slice(&body).map_err(|error| error.to_string())?;
    match response.status.as_str() {
        "claimed" => Ok(Enrollment::Claimed),
        "pending" => Ok(Enrollment::Pending {
            enrollment_id: response.enrollment_id,
            claim_code: response
                .claim_code
                .ok_or_else(|| "pending enrollment did not include a claim code".to_owned())?,
        }),
        _ => Err("device enrollment returned an unknown status".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_payload_does_not_rename_security_fields() {
        let request = RegistrationRequest {
            device_id: "00000000-0000-4000-8000-000000000000",
            device_code: "AB23CD",
            credential: "test-device-credential-abcdefghijklmnopqrstuvwxyz",
        };
        let value = serde_json::to_value(request).expect("serialize request");
        assert_eq!(value["device_code"], "AB23CD");
        assert!(value.get("credential").is_some());
        assert!(value.get("household_id").is_none());
    }

    #[test]
    fn owner_claim_url_prefills_the_visible_claim_code() {
        let client = PairingClient::new("https://frames.example/base").expect("pairing client");
        assert_eq!(
            client.owner_claim_url("AB23CD45").expect("claim URL"),
            "https://frames.example/owner.html?claim_code=AB23CD45"
        );
    }
}
