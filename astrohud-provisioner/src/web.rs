use crate::identity::DeviceIdentity;
use crate::network::{NetworkManager, WifiNetwork};
use serde::Serialize;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const MAX_FORM_BYTES: u64 = 8 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", content = "message", rename_all = "snake_case")]
enum ProvisioningStatus {
    Ready,
    Applying,
    Failed(String),
    Connected,
}

pub fn serve(
    identity: DeviceIdentity,
    manager: NetworkManager,
    networks: Vec<WifiNetwork>,
) -> Result<(), String> {
    let server = Server::http("0.0.0.0:80").map_err(|error| error.to_string())?;
    let status = Arc::new(Mutex::new(ProvisioningStatus::Ready));
    let finished = Arc::new(AtomicBool::new(false));

    while !finished.load(Ordering::Acquire) {
        let Some(request) = server
            .recv_timeout(Duration::from_millis(500))
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        handle_request(
            request,
            &identity,
            &manager,
            &networks,
            Arc::clone(&status),
            Arc::clone(&finished),
        );
    }
    Ok(())
}

fn handle_request(
    mut request: Request,
    identity: &DeviceIdentity,
    manager: &NetworkManager,
    networks: &[WifiNetwork],
    status: Arc<Mutex<ProvisioningStatus>>,
    finished: Arc<AtomicBool>,
) {
    let path = request.url().split('?').next().unwrap_or("/");
    match (request.method(), path) {
        (&Method::Get, "/api/status") => {
            let body = serde_json::to_string(&*status.lock().expect("status lock"))
                .unwrap_or_else(|_| "{\"state\":\"failed\"}".to_owned());
            respond(request, 200, "application/json", body);
        }
        (&Method::Post, "/configure") => {
            let mut body = String::new();
            if request
                .as_reader()
                .take(MAX_FORM_BYTES)
                .read_to_string(&mut body)
                .is_err()
            {
                respond(
                    request,
                    400,
                    "text/plain; charset=utf-8",
                    "Could not read the form.".to_owned(),
                );
                return;
            }
            let fields: std::collections::HashMap<_, _> =
                url::form_urlencoded::parse(body.as_bytes())
                    .into_owned()
                    .collect();
            let selected = fields.get("ssid").map(String::as_str).unwrap_or("");
            let manual = fields
                .get("manual_ssid")
                .map(|value| value.trim())
                .unwrap_or("");
            let ssid = if manual.is_empty() { selected } else { manual };
            let password = fields.get("password").map(String::as_str).unwrap_or("");

            if let Err(message) = validate_credentials(ssid, password) {
                respond(
                    request,
                    400,
                    "text/html; charset=utf-8",
                    message_page("Check the network details", &message),
                );
                return;
            }
            {
                let mut current = status.lock().expect("status lock");
                if matches!(*current, ProvisioningStatus::Applying) {
                    respond(
                        request,
                        409,
                        "text/html; charset=utf-8",
                        message_page("Already connecting", "Please wait for the current attempt."),
                    );
                    return;
                }
                *current = ProvisioningStatus::Applying;
            }

            respond(
                request,
                202,
                "text/html; charset=utf-8",
                connecting_page(ssid),
            );
            let manager = manager.clone();
            let identity = identity.clone();
            let ssid = ssid.to_owned();
            let password = password.to_owned();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(750));
                match manager.apply_candidate(&identity, &ssid, &password) {
                    Ok(()) => {
                        *status.lock().expect("status lock") = ProvisioningStatus::Connected;
                        finished.store(true, Ordering::Release);
                    }
                    Err(error) => {
                        *status.lock().expect("status lock") = ProvisioningStatus::Failed(error);
                    }
                }
            });
        }
        _ => respond(
            request,
            200,
            "text/html; charset=utf-8",
            setup_page(identity, networks, &status.lock().expect("status lock")),
        ),
    }
}

fn validate_credentials(ssid: &str, password: &str) -> Result<(), String> {
    if ssid.is_empty() || ssid.len() > 32 || ssid.chars().any(char::is_control) {
        return Err("Choose a network name between 1 and 32 bytes.".to_owned());
    }
    let is_hex_key = password.len() == 64 && password.bytes().all(|byte| byte.is_ascii_hexdigit());
    let is_passphrase = (8..=63).contains(&password.len())
        && password.is_ascii()
        && !password.chars().any(char::is_control);
    if !password.is_empty() && !is_passphrase && !is_hex_key {
        return Err(
            "Wi-Fi passwords must be 8–63 printable characters or a 64-digit hex key.".to_owned(),
        );
    }
    Ok(())
}

fn respond(request: Request, status: u16, content_type: &str, body: String) {
    let mut response = Response::from_string(body).with_status_code(StatusCode(status));
    for (name, value) in [
        ("Content-Type", content_type),
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        (
            "Content-Security-Policy",
            "default-src 'self'; style-src 'unsafe-inline'",
        ),
    ] {
        response.add_header(Header::from_bytes(name, value).expect("static header"));
    }
    let _ = request.respond(response);
}

fn setup_page(
    identity: &DeviceIdentity,
    networks: &[WifiNetwork],
    status: &ProvisioningStatus,
) -> String {
    let options = networks
        .iter()
        .map(|network| {
            format!(
                "<option value=\"{}\">{} — {}% {}</option>",
                escape_html(&network.ssid),
                escape_html(&network.ssid),
                network.signal,
                escape_html(&network.security)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    let status_message = match status {
        ProvisioningStatus::Failed(message) => format!(
            "<div class=\"notice error\"><strong>Could not connect.</strong> {}</div>",
            escape_html(message)
        ),
        _ => String::new(),
    };
    page_shell(
        "Connect your AstroHUD",
        &format!(
            r#"
            <p class="eyebrow">{ssid}</p>
            <h1>Connect your frame</h1>
            <p>Choose the Wi-Fi network this frame should use. Your phone will disconnect from the AstroHUD setup network while the frame tests it.</p>
            {status_message}
            <form method="post" action="/configure">
              <label for="ssid">Nearby network</label>
              <select id="ssid" name="ssid">{options}</select>
              <label for="manual_ssid">Or enter a network name manually</label>
              <input id="manual_ssid" name="manual_ssid" maxlength="32" autocomplete="off">
              <label for="password">Wi-Fi password</label>
              <input id="password" name="password" type="password" maxlength="63" autocomplete="current-password">
              <button type="submit">Connect frame</button>
            </form>
            <p class="fine">Device {device_code}. Setup remains available if the connection fails.</p>
            "#,
            ssid = escape_html(&identity.setup_ssid),
            device_code = escape_html(&identity.device_code),
        ),
    )
}

fn connecting_page(ssid: &str) -> String {
    page_shell(
        "Connecting",
        &format!(
            "<p class=\"eyebrow\">AstroHUD setup</p><h1>Connecting…</h1><p>The frame is testing <strong>{}</strong>. Your phone will leave this setup network. If the test fails, reconnect to the same AstroHUD setup network and try again.</p>",
            escape_html(ssid)
        ),
    )
}

fn message_page(title: &str, message: &str) -> String {
    page_shell(
        title,
        &format!(
            "<h1>{}</h1><p>{}</p><p><a href=\"/\">Return to setup</a></p>",
            escape_html(title),
            escape_html(message)
        ),
    )
}

fn page_shell(title: &str, content: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{}</title><style>
:root {{ color-scheme: light; font-family: ui-rounded, system-ui, sans-serif; background:#f3eee5; color:#18201b; }}
body {{ margin:0; min-height:100vh; display:grid; place-items:center; }}
main {{ width:min(36rem,calc(100% - 2rem)); margin:1rem; box-sizing:border-box; background:#fffdf8; border:1px solid #d9d0c1; border-radius:1.5rem; padding:clamp(1.4rem,5vw,3rem); box-shadow:0 1rem 3rem #4037251f; }}
h1 {{ font-size:clamp(2rem,8vw,3.5rem); line-height:.95; letter-spacing:-.05em; margin:.25rem 0 1.25rem; }}
p {{ line-height:1.55; }} .eyebrow {{ color:#69756c; font-weight:700; letter-spacing:.08em; text-transform:uppercase; font-size:.78rem; }}
label {{ display:block; font-weight:700; margin:1.1rem 0 .4rem; }} select,input,button {{ width:100%; box-sizing:border-box; border-radius:.8rem; padding:.9rem 1rem; font:inherit; }}
select,input {{ border:1px solid #b8b3a8; background:white; }} button {{ margin-top:1.4rem; border:0; background:#175f45; color:white; font-weight:800; cursor:pointer; }}
.fine {{ color:#69756c; font-size:.82rem; margin-top:1.5rem; }} .notice {{ border-radius:.8rem; padding:.9rem; background:#e8f2ed; }} .error {{ background:#f9e5df; color:#772b20; }}
a {{ color:#175f45; font-weight:700; }}
</style></head><body><main>{}</main></body></html>"#,
        escape_html(title),
        content
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_are_bounded() {
        assert!(validate_credentials("Home", "correct horse").is_ok());
        assert!(validate_credentials("", "correct horse").is_err());
        assert!(validate_credentials("Home", "short").is_err());
        assert!(validate_credentials("Home", "").is_ok());
    }

    #[test]
    fn html_escaping_covers_form_values() {
        assert_eq!(escape_html("<A&B \"C\">"), "&lt;A&amp;B &quot;C&quot;&gt;");
    }
}
