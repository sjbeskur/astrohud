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
use url::Url;

const MAX_FORM_BYTES: u64 = 8 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", content = "message", rename_all = "snake_case")]
enum ProvisioningStatus {
    Ready,
    Applying,
    Failed(String),
    Connected,
}

pub fn bind() -> Result<Server, String> {
    Server::http("0.0.0.0:80").map_err(|error| error.to_string())
}

pub fn serve(
    server: Server,
    identity: DeviceIdentity,
    manager: NetworkManager,
    networks: Vec<WifiNetwork>,
    server_url: &str,
) -> Result<(), String> {
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
            server_url,
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
    server_url: &str,
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
            let onboarding_url = match onboarding_url(server_url, &identity.bootstrap_token) {
                Ok(url) => url,
                Err(message) => {
                    respond(
                        request,
                        500,
                        "text/html; charset=utf-8",
                        message_page("Could not prepare setup", &message),
                    );
                    return;
                }
            };

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
                        "Reconnect to the setup network shown on your screen and try again.",
                    ),
                );
                return;
            }

            respond(
                request,
                202,
                "text/html; charset=utf-8",
                connecting_page(ssid, &onboarding_url),
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
                        eprintln!("household Wi-Fi setup failed: {error}");
                        if let Err(display_error) = setup_display::write_retry(
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
        (&Method::Get, "/") | (&Method::Get, "/index.html") => respond(
            request,
            200,
            "text/html; charset=utf-8",
            setup_page(identity, networks, &status.lock().expect("status lock")),
        ),
        _ => {
            // HTTP connectivity probes should lead straight to the portal.
            let response = Response::empty(StatusCode(302))
                .with_header(
                    Header::from_bytes("Location", "http://10.42.0.1/").expect("static header"),
                )
                .with_header(
                    Header::from_bytes("Cache-Control", "no-store").expect("static header"),
                );
            let _ = request.respond(response);
        }
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
            "default-src 'self'; style-src 'unsafe-inline'; script-src 'unsafe-inline'",
        ),
    ] {
        response.add_header(Header::from_bytes(name, value).expect("static header"));
    }
    let _ = request.respond(response);
}

fn setup_steps(current: usize) -> String {
    let steps = ["Wi-Fi", "Password", "Finish"]
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let number = index + 1;
            format!(
                "<li{}><span class=\"step-number\">{number}</span><span>{label}</span></li>",
                if number == current {
                    " aria-current=\"step\""
                } else {
                    ""
                }
            )
        })
        .collect::<String>();
    format!("<nav class=\"setup-progress\" aria-label=\"Setup progress\"><ol>{steps}</ol></nav>")
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
            "<div class=\"notice error\" role=\"alert\"><strong>Could not connect.</strong> {}</div>",
            escape_html(message)
        ),
        _ => String::new(),
    };
    let manual_mode = networks.is_empty();
    let placeholder = if manual_mode {
        ""
    } else {
        "<option value=\"\" disabled selected>Choose your Wi-Fi network</option>"
    };
    page_shell(
        "Connect your Memory Lane",
        &format!(
            r#"
            {progress}
            <section class="intro">
              <h1>Let's connect your frame.</h1>
              <p>A couple of steps, then you're ready to share photos.</p>
            </section>
            {status_message}
            <section class="instrument-panel">
              <form id="wifi-form" method="post" action="/configure">
                <section id="network-step" aria-labelledby="network-title">
                  <h2 id="network-title" tabindex="-1">Choose your Wi-Fi</h2>
                  <p class="step-copy">Choose the home network your frame will use.</p>
                  <label for="ssid">Wi-Fi network</label>
                  <select id="ssid" name="ssid">{placeholder}{options}<option value="" data-manual{manual_selected}>My network isn't listed…</option></select>
                  <div id="manual-network" {manual_hidden}>
                    <label for="manual_ssid">Network name</label>
                    <input id="manual_ssid" name="manual_ssid" maxlength="32" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="manual-help" {manual_attributes}>
                    <p id="manual-help" class="field-help">{manual_help}</p>
                  </div>
                  {noscript_manual}
                  <button id="next-step" type="button" hidden>Next <span aria-hidden="true">→</span></button>
                </section>
                <section id="password-step" aria-labelledby="password-title">
                  <h2 id="password-title" tabindex="-1">Enter the password</h2>
                  <p class="step-copy">For <strong id="password-network">your home Wi-Fi</strong></p>
                  <label for="password">Wi-Fi password</label>
                  <input id="password" name="password" type="password" maxlength="64" autocomplete="current-password" aria-describedby="password-help">
                  <p id="password-help" class="field-help">Leave blank only if this network has no password.</p>
                  <label class="checkbox-label" id="show-password-label" hidden><input id="show-password" type="checkbox">Show password</label>
                  <p class="field-help">Your phone may disconnect from the setup network while the frame connects.</p>
                  <div class="wizard-actions">
                    <button id="previous-step" class="secondary" type="button" hidden>Back</button>
                    <button id="connect-frame" type="submit">Connect frame <span aria-hidden="true">→</span></button>
                  </div>
                </section>
              </form>
            </section>
            <script>
              const networkSelect = document.querySelector('#ssid');
              const manualGroup = document.querySelector('#manual-network');
              const manualName = document.querySelector('#manual_ssid');
              const wifiPassword = document.querySelector('#password');
              const networkStep = document.querySelector('#network-step');
              const passwordStep = document.querySelector('#password-step');
              const nextStep = document.querySelector('#next-step');
              const previousStep = document.querySelector('#previous-step');
              const showPassword = document.querySelector('#show-password');
              function updateNetwork(clearPassword = false) {{
                const manual = networkSelect.options[networkSelect.selectedIndex].hasAttribute('data-manual');
                manualGroup.hidden = !manual;
                manualName.disabled = !manual;
                manualName.required = manual;
                networkSelect.required = !manual;
                const name = manual ? manualName.value.trim() : networkSelect.value;
                manualName.setCustomValidity(manual && !name ? 'Enter your network name.'
                  : manual && new TextEncoder().encode(name).length > 32 ? 'Use a network name up to 32 bytes long.' : '');
                document.querySelector('#password-network').textContent = name || 'your home Wi-Fi';
                nextStep.disabled = !name;
                if (clearPassword) wifiPassword.value = '';
              }}
              function showStep(number) {{
                networkStep.hidden = number !== 1;
                passwordStep.hidden = number !== 2;
                document.querySelectorAll('.setup-progress li').forEach((step, index) => {{
                  if (index + 1 === number) step.setAttribute('aria-current', 'step');
                  else step.removeAttribute('aria-current');
                }});
                const title = document.querySelector(number === 1 ? '#network-title' : '#password-title');
                title.focus();
                title.scrollIntoView({{block: 'nearest'}});
              }}
              function advance() {{
                updateNetwork();
                if (!networkSelect.reportValidity() || (!manualName.disabled && !manualName.reportValidity())) return;
                showStep(2);
              }}
              networkSelect.addEventListener('change', () => {{
                updateNetwork(true);
                if (!manualGroup.hidden) manualName.focus();
              }});
              manualName.addEventListener('input', () => updateNetwork(true));
              manualName.addEventListener('keydown', event => {{
                if (event.key === 'Enter') {{ event.preventDefault(); advance(); }}
              }});
              nextStep.addEventListener('click', advance);
              previousStep.addEventListener('click', () => showStep(1));
              showPassword.addEventListener('change', () => {{ wifiPassword.type = showPassword.checked ? 'text' : 'password'; }});
              document.querySelector('#wifi-form').addEventListener('submit', event => {{
                if (!networkStep.hidden) {{ event.preventDefault(); advance(); return; }}
                const button = document.querySelector('#connect-frame');
                button.disabled = true;
                button.textContent = 'Connecting…';
              }});
              nextStep.hidden = false;
              previousStep.hidden = false;
              document.querySelector('#show-password-label').hidden = false;
              passwordStep.hidden = true;
              updateNetwork();
            </script>
            <p class="fine setup-link">Connected to {ssid} · Frame {device_code}</p>
            "#,
            progress = setup_steps(1),
            ssid = escape_html(&identity.setup_ssid),
            device_code = escape_html(&identity.device_code),
            manual_selected = if manual_mode { " selected" } else { "" },
            manual_hidden = if manual_mode { "" } else { "hidden" },
            manual_attributes = if manual_mode { "required" } else { "disabled" },
            noscript_manual = if manual_mode {
                ""
            } else {
                "<noscript><label for=\"manual-fallback\">Network not listed? Enter its name</label><input id=\"manual-fallback\" name=\"manual_ssid\" maxlength=\"32\" autocomplete=\"off\" autocapitalize=\"none\"></noscript>"
            },
            manual_help = if manual_mode {
                "The network list is unavailable. Enter your Wi-Fi network name exactly."
            } else {
                "Enter the exact name of your hidden or unlisted Wi-Fi network."
            },
        ),
    )
}

fn onboarding_url(server_url: &str, bootstrap_token: &str) -> Result<String, String> {
    let mut url = Url::parse(server_url).map_err(|error| error.to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Memory Lane server URL must use HTTP or HTTPS".to_owned());
    }
    url.path_segments_mut()
        .map_err(|_| "Memory Lane server URL cannot be a base URL".to_owned())?
        .clear()
        .extend(["onboard.html"]);
    url.set_fragment(Some(bootstrap_token));
    Ok(url.into())
}

fn connecting_page(ssid: &str, onboarding_url: &str) -> String {
    page_shell(
        "Connecting",
        &format!(
            r#"{progress}<section class="message-panel">
              <p class="signal-label"><span></span> Network link / Testing</p>
              <h1 id="connection-title">Connecting…</h1>
              <p>The frame is testing <strong>{}</strong>. This can take about 35 seconds.</p>
              <p id="connection-result" role="status">Watch your television for the result. Your phone may disconnect from the temporary setup network during this test.</p>
              <div id="connection-next">
                <p>When the television shows <strong>Wi-Fi saved</strong> or <strong>Finishing setup</strong>, connect your phone to home Wi-Fi or cellular service, then continue below.</p>
                <p><a id="continue-setup" class="signal-button" href="{}">Continue setup <span aria-hidden="true">→</span></a></p>
              </div>
              <p>If the television shows the setup QR again, rejoin that setup network and return below to check the network name and password.</p>
              <p><a id="retry-setup" href="http://10.42.0.1/">Return to Wi-Fi setup</a></p>
            </section>
            <script>
              async function checkConnection() {{
                const controller = new AbortController();
                const timeout = setTimeout(() => controller.abort(), 3000);
                try {{
                  const response = await fetch('/api/status', {{cache: 'no-store', signal: controller.signal}});
                  if (!response.ok) throw new Error('Setup connection unavailable');
                  const status = await response.json();
                  if (status.state === 'failed' || status.state === 'ready') {{
                    document.querySelector('#connection-title').textContent = 'Try connecting again';
                    document.querySelector('#connection-result').textContent = status.state === 'failed'
                      ? status.message
                      : 'The setup network has restarted. Return to Wi-Fi setup to try again.';
                    document.querySelector('#connection-next').hidden = true;
                    document.querySelector('#retry-setup').classList.add('signal-button');
                    return;
                  }}
                  if (status.state === 'connected') {{
                    document.querySelector('#connection-title').textContent = 'Wi-Fi saved';
                    document.querySelector('#connection-result').textContent = 'Connect your phone to home Wi-Fi or cellular service, then tap Continue setup.';
                    return;
                  }}
                }} catch (_) {{
                  // Losing the AP is expected during a connection attempt, but
                  // does not prove success. Keep the TV instructions visible.
                }} finally {{ clearTimeout(timeout); }}
                setTimeout(checkConnection, 2000);
              }}
              setTimeout(checkConnection, 1500);
            </script>"#,
            escape_html(ssid),
            escape_html(onboarding_url),
            progress = setup_steps(3),
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
[hidden] {{ display:none!important; }}
body {{ margin:0; min-height:100vh; color:var(--text); background:radial-gradient(circle at 82% 0%,#252b48 0,transparent 30rem),var(--void); }}
.nav {{ border-bottom:1px solid var(--line); background:#070910eb; }} .nav-row {{ display:flex; width:min(42rem,calc(100% - 2rem)); min-height:58px; margin:auto; align-items:center; justify-content:space-between; gap:1rem; }}
.brand {{ display:inline-flex; min-height:42px; align-items:center; gap:.65rem; margin-left:-1rem; border-radius:0 22px 22px 0; padding:.4rem 1.15rem .4rem 1rem; color:#17120d; background:var(--amber); font-size:.75rem; font-weight:900; letter-spacing:.13em; }}
.mark {{ display:grid; width:28px; height:28px; place-items:center; border:2px solid currentcolor; border-radius:50%; font-size:.55rem; letter-spacing:-.04em; }}
.nav-state,.signal-label,.instrument-heading,.status-line {{ font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace; font-weight:800; letter-spacing:.11em; text-transform:uppercase; }}
.nav-state {{ display:flex; align-items:center; gap:.5rem; color:var(--muted); font-size:.55rem; }} .nav-state i,.status-line i {{ width:7px; height:7px; border-radius:50%; background:var(--green); box-shadow:0 0 10px #69d59b80; }}
main {{ width:min(42rem,calc(100% - 2rem)); margin:auto; padding:2rem 0 3rem; }}
.signal-label {{ display:flex; align-items:center; gap:.65rem; margin:0 0 1rem; color:var(--amber); font-size:.62rem; }} .signal-label span {{ width:28px; height:2px; background:currentcolor; }}
h1 {{ margin:0 0 1.25rem; font-size:clamp(1.75rem,6vw,2.5rem); font-weight:650; letter-spacing:-.065em; line-height:1.12; }} h1 em {{ color:var(--lavender); font-family:Iowan Old Style,Baskerville,"Times New Roman",serif; font-weight:400; }}
p {{ line-height:1.65; }} .intro>p:last-child,.message-panel>p {{ color:var(--muted); }}
.instrument-panel,.message-panel {{ position:relative; margin-top:2rem; border:1px solid var(--line); border-radius:20px; padding:clamp(1.25rem,5vw,2rem); background:linear-gradient(145deg,var(--raised),var(--surface)); box-shadow:0 12px 36px #0003; }}
.instrument-heading {{ display:flex; justify-content:space-between; gap:1rem; margin-bottom:1.5rem; color:var(--muted); font-size:.55rem; }} .instrument-heading b {{ color:var(--salmon); }}
label {{ display:block; margin:1rem 0 .4rem; font-size:.95rem; font-weight:650; }} select,input,button {{ width:100%; min-height:50px; border-radius:12px; padding:.75rem .9rem; color:var(--text); font:inherit; }}
select,input {{ border:1px solid #465269; background:#0b0f18; }} button,.signal-button {{ display:flex; align-items:center; justify-content:space-between; margin-top:1.4rem; border:0; padding:.8rem 1.2rem; color:#17120d; background:var(--amber); font-size:1rem; font-weight:750; text-decoration:none; cursor:pointer; }}
.field-help {{ margin:.45rem 0 0; color:var(--muted); font-size:.78rem; line-height:1.5; }} button:disabled,input:disabled {{ opacity:.55; cursor:default; }}
:focus-visible {{ outline:3px solid var(--blue); outline-offset:3px; }}
.fine {{ margin-top:1.3rem; color:var(--muted); font-size:.75rem; }} .notice {{ margin:1.5rem 0; border:1px solid var(--line); border-radius:4px 18px 18px 4px; padding:.9rem 1rem; background:#171e2b; }} .error {{ color:#ffc0b5; }}
.status-line {{ display:flex; align-items:center; gap:.55rem; margin:1px 0 0; border:1px solid var(--line); border-radius:5px 20px 20px 5px; padding:1rem 1.2rem; color:var(--green); background:#0b0e16; font-size:.56rem; }} .status-line.pending {{ color:var(--blue); }} .status-line.pending i {{ background:var(--blue); box-shadow:0 0 10px #70a9d680; }}
.message-panel {{ margin-top:0; }} .message-panel .signal-button {{ margin-top:2rem; }} strong {{ color:var(--text); }}

h2 {{ margin:0 0 .65rem; font-size:1.3rem; letter-spacing:-.025em; }}
.step-copy {{ margin:.5rem 0 1.25rem; color:var(--muted); overflow-wrap:anywhere; }}
.setup-progress {{ margin:0 0 2rem; }} .setup-progress ol {{ display:flex; gap:.8rem; margin:0; padding:0; list-style:none; }}
.setup-progress li {{ flex:1; display:flex; align-items:center; gap:.5rem; color:var(--muted); font-size:.8rem; }}
.step-number {{ display:grid; place-items:center; width:28px; height:28px; flex-shrink:0; border:1px solid var(--line); border-radius:50%; }}
.setup-progress [aria-current=step] {{ color:var(--text); font-weight:750; }} .setup-progress [aria-current=step] .step-number {{ border-color:var(--amber); color:#17120d; background:var(--amber); }}
.wizard-actions {{ display:flex; align-items:center; gap:.8rem; }} .wizard-actions .secondary {{ width:auto; justify-content:center; border:1px solid var(--line); color:var(--text); background:transparent; }}
.checkbox-label {{ display:flex; align-items:center; gap:.65rem; margin:1rem 0; font-weight:400; }} .checkbox-label input {{ width:20px; min-height:20px; height:20px; accent-color:var(--amber); }}
.setup-link {{ overflow-wrap:anywhere; text-align:center; }}
@media(max-width:520px) {{ .nav-row,main {{ width:min(100% - 1.5rem,42rem); }} .brand {{ margin-left:-.75rem;padding-left:.75rem; }} .nav-state {{ letter-spacing:.04em; }} .instrument-heading {{ flex-direction:column; }} }}
@media(prefers-reduced-motion:reduce) {{ *,*::before,*::after {{ transition-duration:.01ms!important; }} }}
</style></head><body><header class="nav"><div class="nav-row"><div class="brand"><span class="mark">ML</span> Memory Lane</div><div class="nav-state"><i></i> Setup link / Active</div></div></header><main>{}</main></body></html>"##,
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
        let page = connecting_page(
            "Grandma's Wi-Fi",
            "https://app.astrohud.com/onboard.html#private-bootstrap-token",
        );
        assert!(page.contains("Watch your television for the result."));
        assert!(page.contains("Continue setup"));
        assert!(page.contains("onboard.html#private-bootstrap-token"));
        assert!(page.contains("about 35 seconds"));
        assert!(page.contains("Return to Wi-Fi setup"));
        assert!(!page.contains(".click()"));
    }

    #[test]
    fn onboarding_secret_stays_in_the_url_fragment() {
        assert_eq!(
            onboarding_url(
                "https://app.astrohud.com/base",
                "private-bootstrap-token-abcdefghijklmnopqrstuvwxyz"
            )
            .expect("onboarding URL"),
            "https://app.astrohud.com/onboard.html#private-bootstrap-token-abcdefghijklmnopqrstuvwxyz"
        );
    }

    #[test]
    #[ignore = "writes a manual preview artifact to /tmp"]
    fn write_portal_preview() {
        let identity = DeviceIdentity {
            device_code: "AB23CD".to_owned(),
            setup_ssid: "Memory Lane-AB23CD".to_owned(),
            setup_password: "23456789ABCDEFGH".to_owned(),
            device_id: "00000000-0000-4000-8000-000000000000".to_owned(),
            device_credential: "test-device-credential-abcdefghijklmnopqrstuvwxyz".to_owned(),
            bootstrap_token: "test-bootstrap-token-abcdefghijklmnopqrstuvwxyz0123456789".to_owned(),
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
        std::fs::write(
            "/tmp/astrohud-portal-empty-preview.html",
            setup_page(&identity, &[], &ProvisioningStatus::Ready),
        )
        .expect("write empty-list preview");
        std::fs::write(
            "/tmp/astrohud-connecting-preview.html",
            connecting_page(
                "Grandma's Wi-Fi",
                "https://app.astrohud.com/onboard.html#test-bootstrap",
            ),
        )
        .expect("write connecting preview");
    }
}
