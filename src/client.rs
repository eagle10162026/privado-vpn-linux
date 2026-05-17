use crate::daemon::proto::{Request, Response, ProvisionedServer, VpnStatus, SOCKET_PATH};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const HTTP_BASE: &str = "http://127.10.0.18:1600";

fn http_get(path: &str) -> Result<String, String> {
    let url = format!("{HTTP_BASE}{path}");
    let resp = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("http client: {e}"))?
        .get(&url)
        .send()
        .map_err(|e| format!("http get {url}: {e}"))?;
    resp.text().map_err(|e| format!("http body: {e}"))
}

fn http_post(path: &str, body: &impl serde::Serialize) -> Result<String, String> {
    let url = format!("{HTTP_BASE}{path}");
    let resp = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client: {e}"))?
        .post(&url)
        .json(body)
        .send()
        .map_err(|e| format!("http post {url}: {e}"))?;
    resp.text().map_err(|e| format!("http body: {e}"))
}

fn call_socket(req: &Request) -> Result<Response, String> {
    let mut stream = UnixStream::connect(SOCKET_PATH).map_err(|e| {
        format!(
            "cannot reach daemon at {SOCKET_PATH}: {e}\n\
             Is the privado-vpn.service running? Try: sudo systemctl start privado-vpn"
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    let mut bytes = serde_json::to_vec(req).map_err(|e| format!("serialize: {e}"))?;
    bytes.push(b'\n');
    stream.write_all(&bytes).map_err(|e| format!("write: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| format!("read: {e}"))?;
    if line.is_empty() {
        return Err("daemon closed connection without responding".into());
    }
    serde_json::from_str(line.trim()).map_err(|e| format!("parse '{line}': {e}"))
}

pub fn status() -> Result<VpnStatus, String> {
    if let Ok(text) = http_get("/status") {
        if let Ok(s) = serde_json::from_str::<VpnStatus>(&text) {
            return Ok(s);
        }
    }
    match call_socket(&Request::Status)? {
        Response::Ok { status } => Ok(status),
        Response::Err { message, .. } => Err(message),
        _ => Err("unexpected response shape for status".into()),
    }
}

pub fn servers() -> Result<Vec<ProvisionedServer>, String> {
    if let Ok(text) = http_get("/servers") {
        if let Ok(Response::Servers { entries }) = serde_json::from_str::<Response>(&text) {
            return Ok(entries);
        }
    }
    match call_socket(&Request::Servers)? {
        Response::Servers { entries } => Ok(entries),
        Response::Err { message, .. } => Err(message),
        _ => Err("unexpected response shape for servers".into()),
    }
}

#[derive(serde::Serialize)]
struct ConnectBody {
    pin: String,
    country: String,
}

pub fn connect_vpn(pin: &str, country: &str) -> Result<VpnStatus, String> {
    let body = ConnectBody { pin: pin.to_string(), country: country.to_string() };
    if let Ok(text) = http_post("/connect", &body) {
        if let Ok(Response::Ok { status }) = serde_json::from_str::<Response>(&text) {
            return Ok(status);
        }
        if let Ok(Response::Err { message, .. }) = serde_json::from_str::<Response>(&text) {
            return Err(message);
        }
    }
    match call_socket(&Request::Connect { pin: pin.to_string(), country: country.to_string() })? {
        Response::Ok { status } => Ok(status),
        Response::Err { message, .. } => Err(message),
        _ => Err("unexpected response shape for connect".into()),
    }
}

pub fn disconnect_vpn(pin: &str) -> Result<VpnStatus, String> {
    #[derive(serde::Serialize)]
    struct DisconnectBody { pin: String }
    let body = DisconnectBody { pin: pin.to_string() };
    if let Ok(text) = http_post("/disconnect", &body) {
        if let Ok(Response::Ok { status }) = serde_json::from_str::<Response>(&text) {
            return Ok(status);
        }
        if let Ok(Response::Err { message, .. }) = serde_json::from_str::<Response>(&text) {
            return Err(message);
        }
    }
    match call_socket(&Request::Disconnect { pin: pin.to_string() })? {
        Response::Ok { status } => Ok(status),
        Response::Err { message, .. } => Err(message),
        _ => Err("unexpected response shape for disconnect".into()),
    }
}
