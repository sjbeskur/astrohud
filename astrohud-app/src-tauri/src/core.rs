use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{Client, Method, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

const MAX_BODY: usize = 25 * 1024 * 1024;

#[derive(Clone)]
pub struct Session {
    pub server: Url,
    client: Client,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub server: String,
    pub local: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRequest {
    pub path: String,
    pub method: String,
    pub content_type: Option<String>,
    pub body_base64: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse {
    pub status: u16,
    pub content_type: String,
    pub body_base64: String,
}

pub fn local(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    )
}

impl Session {
    pub fn new(value: &str) -> Result<Self, String> {
        let server = Url::parse(value.trim()).map_err(|_| "Enter a complete server URL.")?;
        if server.host_str().is_none()
            || !server.username().is_empty()
            || server.password().is_some()
            || server.query().is_some()
            || server.fragment().is_some()
            || server.path() != "/"
            || !(server.scheme() == "https" || (server.scheme() == "http" && local(&server)))
        {
            return Err("Use an HTTPS server origin, or HTTP localhost for development.".into());
        }
        let client = Client::builder()
            .user_agent(concat!(
                "MemoryLane-Desktop-Draft/",
                env!("CARGO_PKG_VERSION")
            ))
            .cookie_store(true)
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|_| "Could not start the connection.")?;
        Ok(Self { server, client })
    }

    pub fn info(&self) -> ConnectionInfo {
        ConnectionInfo {
            server: self.server.as_str().trim_end_matches('/').into(),
            local: local(&self.server),
        }
    }

    fn target(&self, path: &str) -> Result<Url, String> {
        if !path.starts_with("/api/beta/") || path.contains('#') {
            return Err("Only Memory Lane API requests are allowed.".into());
        }
        let url = self.server.join(path).map_err(|_| "Invalid API path.")?;
        if url.origin() != self.server.origin() || !url.path().starts_with("/api/beta/") {
            return Err("The request must stay on the connected server.".into());
        }
        Ok(url)
    }

    pub async fn request(&self, input: ApiRequest) -> Result<ApiResponse, String> {
        let target = self.target(&input.path)?;
        let method = match input.method.as_str() {
            "GET" => Method::GET,
            "POST" => Method::POST,
            "DELETE" => Method::DELETE,
            _ => return Err("Unsupported request method.".into()),
        };
        let mut request = self.client.request(method, target);
        if let Some(kind) = input.content_type {
            if !(kind.starts_with("application/json") || kind.starts_with("multipart/form-data;")) {
                return Err("Unsupported request content type.".into());
            }
            request = request.header("Content-Type", kind);
        }
        if let Some(encoded) = input.body_base64 {
            if encoded.len() > MAX_BODY.div_ceil(3) * 4 {
                return Err("Photo request is too large.".into());
            }
            let body = STANDARD
                .decode(encoded)
                .map_err(|_| "Invalid request body.")?;
            if body.len() > MAX_BODY {
                return Err("Photo request is too large.".into());
            }
            request = request.body(body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "Could not reach the server. Check your connection and try again.")?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "The server response was interrupted.")?
        {
            if body.len().saturating_add(chunk.len()) > MAX_BODY {
                return Err("The server response is too large.".into());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(ApiResponse {
            status,
            content_type,
            body_base64: STANDARD.encode(body),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn incoming(stream: &std::net::TcpStream) -> String {
        use std::io::{BufRead, Read};
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let length = head
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|n| n.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        head.push_str(&String::from_utf8_lossy(&body));
        head
    }
    #[tokio::test]
    async fn cookies_stay_native_and_a_new_session_has_no_previous_access() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = incoming(&stream);
                if step == 1 {
                    assert!(request.contains("astrohud_owner=fixture"));
                } else {
                    assert!(!request.contains("astrohud_owner=fixture"));
                }
                let cookie = if step == 0 {
                    "Set-Cookie: astrohud_owner=fixture; HttpOnly; Path=/\r\n"
                } else {
                    ""
                };
                write!(stream,"HTTP/1.1 200 OK\r\n{cookie}Content-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
            }
        });
        let session = Session::new(&url).unwrap();
        for active in [&session, &session, &Session::new(&url).unwrap()] {
            let response = active
                .request(ApiRequest {
                    path: "/api/beta/owner".into(),
                    method: "GET".into(),
                    content_type: None,
                    body_base64: None,
                })
                .await
                .unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(STANDARD.decode(response.body_base64).unwrap(), b"{}");
            assert_eq!(response.content_type, "application/json");
        }
        server.join().unwrap();
    }
    #[tokio::test]
    async fn redirects_cannot_forward_a_session_to_another_server() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            incoming(&stream);
            write!(stream,"HTTP/1.1 302 Found\r\nLocation: https://elsewhere.invalid/api/beta/owner\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let response = Session::new(&url)
            .unwrap()
            .request(ApiRequest {
                path: "/api/beta/owner".into(),
                method: "GET".into(),
                content_type: None,
                body_base64: None,
            })
            .await
            .unwrap();
        assert_eq!(response.status, 302);
        server.join().unwrap();
    }
    #[test]
    fn server_and_request_boundaries_prevent_credential_forwarding() {
        for server in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com/?token=x",
            "file:///tmp",
        ] {
            assert!(Session::new(server).is_err());
        }
        let session = Session::new("https://app.astrohud.com").unwrap();
        assert!(session.target("/api/beta/owner").is_ok());
        for path in [
            "https://elsewhere.test/api/beta/owner",
            "//elsewhere.test",
            "/api/beta/../../private",
            "/api/beta/%2e%2e/%2e%2e/private",
            "/api/health",
            "/api/beta/owner#secret",
        ] {
            assert!(session.target(path).is_err(), "{path}");
        }
        assert!(Session::new("http://127.0.0.1:18116").unwrap().info().local);
    }
}
