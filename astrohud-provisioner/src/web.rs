use crate::identity::DeviceIdentity;
use crate::network::{NetworkManager, WifiNetwork};
use crate::setup_display;
use serde::Serialize;
use std::io::Read;
use std::path::Path;
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

            if let Err(error) = setup_display::write_connecting(
                identity,
                ssid,
                Path::new(setup_display::SETUP_SCREEN_PATH),
            ) {
                *status.lock().expect("status lock") = ProvisioningStatus::Failed(error.clone());
                respond(
                    request,
                    500,
                    "text/html; charset=utf-8",
                    message_page(
                        "Could not update the television",
                        "Reconnect to the AstroHUD network and try again.",
                    ),
                );
                return;
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
                        if let Err(display_error) = setup_display::write(
                            &identity,
                            Path::new(setup_display::SETUP_SCREEN_PATH),
                        ) {
                            eprintln!("could not restore setup screen: {display_error}");
                        }
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
            <section class="intro">
              <p class="signal-label"><span></span> Setup / {device_code}</p>
              <h1>Bring your frame <em>online.</em></h1>
              <p>Choose the Wi-Fi network this frame should use. Your phone will leave the temporary AstroHUD link while the frame tests it.</p>
            </section>
            {status_message}
            <section class="instrument-panel">
              <div class="instrument-heading"><span><b>01</b> / Network link</span><span>Protected setup</span></div>
              <form method="post" action="/configure">
                <label for="ssid">Nearby network</label>
                <select id="ssid" name="ssid">{options}</select>
                <label for="manual_ssid">Or enter a network name manually</label>
                <input id="manual_ssid" name="manual_ssid" maxlength="32" autocomplete="off">
                <label for="password">Wi-Fi password</label>
                <input id="password" name="password" type="password" maxlength="63" autocomplete="current-password">
                <button type="submit">Connect frame <span aria-hidden="true">→</span></button>
              </form>
            </section>
            <p class="status-line"><i></i> Setup link active / {ssid}</p>
            <p class="fine">If the connection fails, reconnect to this AstroHUD setup link and try again.</p>
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
            "<section class=\"message-panel\"><p class=\"signal-label\"><span></span> Network link / Testing</p><h1>Connecting<span class=\"ellipsis\">…</span></h1><p>The frame is testing <strong>{}</strong>. This page will disconnect when the temporary AstroHUD network closes.</p><p><strong>Watch the television next.</strong> It will show a claim QR code when the frame is online. If needed, reconnect your phone to your home Wi-Fi before scanning it.</p><p class=\"status-line pending\"><i></i> Connection test active / About 30 seconds</p></section>",
            escape_html(ssid)
        ),
    )
}

fn message_page(title: &str, message: &str) -> String {
    page_shell(
        title,
        &format!(
            "<section class=\"message-panel\"><p class=\"signal-label\"><span></span> Setup / Attention</p><h1>{}</h1><p>{}</p><p><a class=\"signal-button\" href=\"/\">Return to setup <span aria-hidden=\"true\">→</span></a></p></section>",
            escape_html(title),
            escape_html(message)
        ),
    )
}

fn page_shell(title: &str, content: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="theme-color" content="#070910">
<title>{}</title><style>
:root {{ color-scheme:dark; --void:#070910; --surface:#101520; --raised:#171e2b; --line:#303a4b; --text:#edf2f7; --muted:#99a6b8; --amber:#efb46a; --salmon:#e98c77; --lavender:#ad96d8; --blue:#70a9d6; --green:#69d59b; font-family:Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif; }}
* {{ box-sizing:border-box; }} html {{ min-height:100%; background:var(--void); }}
body {{ margin:0; min-height:100vh; color:var(--text); background:radial-gradient(circle at 82% 0%,#252b48 0,transparent 30rem),var(--void); }}
body::before {{ position:fixed; inset:0 auto 0 0; width:5px; background:linear-gradient(var(--amber) 0 22%,var(--salmon) 22% 48%,var(--lavender) 48% 74%,var(--blue) 74%); content:""; }}
.nav {{ border-bottom:1px solid var(--line); background:#070910eb; }} .nav-row {{ display:flex; width:min(42rem,calc(100% - 2rem)); min-height:58px; margin:auto; align-items:center; justify-content:space-between; gap:1rem; }}
.brand {{ display:inline-flex; min-height:42px; align-items:center; gap:.65rem; margin-left:-1rem; border-radius:0 22px 22px 0; padding:.4rem 1.15rem .4rem 1rem; color:#17120d; background:var(--amber); font-size:.75rem; font-weight:900; letter-spacing:.13em; }}
.mark {{ display:grid; width:28px; height:28px; place-items:center; border:2px solid currentcolor; border-radius:50%; font-size:.55rem; letter-spacing:-.04em; }}
.nav-state,.signal-label,.instrument-heading,.status-line {{ font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; font-weight:800; letter-spacing:.11em; text-transform:uppercase; }}
.nav-state {{ display:flex; align-items:center; gap:.5rem; color:var(--muted); font-size:.55rem; }} .nav-state i,.status-line i {{ width:7px; height:7px; border-radius:50%; background:var(--green); box-shadow:0 0 10px #69d59b80; }}
main {{ width:min(42rem,calc(100% - 2rem)); margin:auto; padding:clamp(3rem,10vw,5.5rem) 0 4rem; }}
.signal-label {{ display:flex; align-items:center; gap:.65rem; margin:0 0 1rem; color:var(--amber); font-size:.62rem; }} .signal-label span {{ width:28px; height:2px; background:currentcolor; }}
h1 {{ margin:0 0 1.25rem; font-size:clamp(2.8rem,12vw,5rem); font-weight:650; letter-spacing:-.065em; line-height:.94; }} h1 em {{ color:var(--lavender); font-family:Iowan Old Style,Baskerville,"Times New Roman",serif; font-weight:400; }}
p {{ line-height:1.65; }} .intro>p:last-child,.message-panel>p {{ color:var(--muted); }}
.instrument-panel,.message-panel {{ position:relative; margin-top:2rem; border:1px solid var(--line); border-left:7px solid var(--salmon); border-radius:6px 26px 26px 6px; padding:clamp(1.25rem,5vw,2rem); background:linear-gradient(145deg,var(--raised),var(--surface)); box-shadow:14px 14px 0 #0d111b; }}
.instrument-panel::before,.message-panel::before {{ position:absolute; top:-1px; left:-7px; width:7px; height:72px; border-radius:5px 0 0; background:var(--amber); content:""; }}
.instrument-heading {{ display:flex; justify-content:space-between; gap:1rem; margin-bottom:1.5rem; color:var(--muted); font-size:.55rem; }} .instrument-heading b {{ color:var(--salmon); }}
label {{ display:block; margin:1rem 0 .4rem; font-size:.78rem; font-weight:800; }} select,input,button {{ width:100%; min-height:50px; border-radius:5px 17px 17px 5px; padding:.75rem .9rem; color:var(--text); font:inherit; }}
select,input {{ border:1px solid #465269; background:#0b0f18; }} button,.signal-button {{ display:flex; align-items:center; justify-content:space-between; margin-top:1.4rem; border:0; padding:.8rem 1.2rem; color:#17120d; background:var(--lavender); font-size:.8rem; font-weight:900; text-decoration:none; cursor:pointer; }}
.fine {{ margin-top:1.3rem; color:var(--muted); font-size:.75rem; }} .notice {{ margin:1.5rem 0; border:1px solid var(--line); border-left:5px solid var(--salmon); border-radius:4px 18px 18px 4px; padding:.9rem 1rem; background:#171e2b; }} .error {{ color:#ffc0b5; }}
.status-line {{ display:flex; align-items:center; gap:.55rem; margin:1px 0 0; border:1px solid var(--line); border-radius:5px 20px 20px 5px; padding:1rem 1.2rem; color:var(--green); background:#0b0e16; font-size:.56rem; }} .status-line.pending {{ color:var(--blue); }} .status-line.pending i {{ background:var(--blue); box-shadow:0 0 10px #70a9d680; }}
.message-panel {{ margin-top:0; }} .message-panel .signal-button {{ margin-top:2rem; }} strong {{ color:var(--text); }}
@media(max-width:520px) {{ .nav-row,main {{ width:min(100% - 1.5rem,42rem); }} .brand {{ margin-left:-.75rem;padding-left:.75rem; }} .nav-state {{ letter-spacing:.04em; }} .instrument-heading {{ flex-direction:column; }} }}
@media(prefers-reduced-motion:reduce) {{ *,*::before,*::after {{ transition-duration:.01ms!important; }} }}
</style></head><body><header class="nav"><div class="nav-row"><div class="brand"><span class="mark">AH</span> ASTROHUD</div><div class="nav-state"><i></i> Setup link / Active</div></div></header><main>{}</main></body></html>"##,
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

    #[test]
    fn connecting_page_explains_the_phone_to_television_handoff() {
        let page = connecting_page("Grandma's Wi-Fi");
        assert!(page.contains("Watch the television next."));
        assert!(page.contains("claim QR code"));
        assert!(page.contains("About 30 seconds"));
    }

    #[test]
    #[ignore = "writes a manual preview artifact to /tmp"]
    fn write_portal_preview() {
        let identity = DeviceIdentity {
            device_code: "AB23CD".to_owned(),
            setup_ssid: "AstroHUD-AB23CD".to_owned(),
            setup_password: "23456789ABCDEFGH".to_owned(),
            device_id: "00000000-0000-4000-8000-000000000000".to_owned(),
            device_credential: "test-device-credential-abcdefghijklmnopqrstuvwxyz".to_owned(),
        };
        let networks = vec![
            WifiNetwork {
                ssid: "Grandma's Wi-Fi".to_owned(),
                signal: 92,
                security: "WPA2".to_owned(),
            },
            WifiNetwork {
                ssid: "Guest Network".to_owned(),
                signal: 68,
                security: "WPA2".to_owned(),
            },
        ];
        std::fs::write(
            "/tmp/astrohud-portal-preview.html",
            setup_page(&identity, &networks, &ProvisioningStatus::Ready),
        )
        .expect("write portal preview");
    }
}
