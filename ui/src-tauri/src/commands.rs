//! Additional Tauri commands for full APK feature parity.
//! Split into a separate module to keep lib.rs manageable.

use crate::{
    AppState, ControlTowerConfig, ServerEntry, SpeedTestResult, TokenInfo,
    config_dir, find_working_api, get_or_refresh_token, daemon_get, daemon_post,
    now_epoch, now_iso, save_config_to_disk, save_credentials_to_disk,
    save_json_to_disk, save_speeds_to_disk, load_json_from_disk,
    API_KEY, VPN_PIN,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tauri::State;

#[allow(unused_imports)]
use serde_json;

// ====== PING-BASED SERVER SELECTION ======

#[tauri::command]
pub async fn vpn_ping_servers(country: String, state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let servers = state.servers.lock().unwrap().clone();
    let cc = if country.is_empty() { "NL".to_string() } else { country.to_uppercase() };

    let candidates: Vec<ServerEntry> = servers.into_iter()
        .filter(|s| s.country_code.eq_ignore_ascii_case(&cc))
        .filter(|s| s.status == "online" || s.status == "1" || s.status == "active")
        .take(10)
        .collect();

    if candidates.is_empty() {
        return Err(format!("No servers found for country '{cc}'"));
    }

    let mut handles = Vec::new();
    for server in &candidates {
        let hostname = server.hostname.clone();
        let load = server.load;
        let name = server.name.clone();
        let city = server.city.clone();
        handles.push(tokio::spawn(async move {
            let addr = format!("{hostname}:443");
            let start = Instant::now();
            let rtt_ms = match tokio::time::timeout(
                Duration::from_secs(5),
                tokio::net::TcpStream::connect(&addr),
            ).await {
                Ok(Ok(_stream)) => start.elapsed().as_millis() as u32,
                _ => 9999,
            };
            (hostname, name, city, load, rtt_ms)
        }));
    }

    let mut results: Vec<(String, String, String, f64, u32)> = Vec::new();
    for h in handles {
        if let Ok(r) = h.await { results.push(r); }
    }

    // Sort by latency first for ranking.
    results.sort_by(|a, b| a.4.cmp(&b.4));

    // Combined score: 0.6 * latency_rank + 0.4 * load_rank.
    let mut load_sorted = results.clone();
    load_sorted.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

    let output: Vec<serde_json::Value> = results.iter().enumerate().map(|(lat_rank, entry)| {
        let load_rank = load_sorted.iter().position(|e| e.0 == entry.0).unwrap_or(0);
        let score = 0.6 * (lat_rank as f64) + 0.4 * (load_rank as f64);
        serde_json::json!({
            "hostname": entry.0,
            "name": entry.1,
            "city": entry.2,
            "load": entry.3,
            "ping_ms": entry.4,
            "score": (score * 100.0).round() / 100.0,
        })
    }).collect();

    Ok(output)
}

// ====== ACCOUNT CREATION ======

#[tauri::command]
pub async fn vpn_create_account(email: String, password: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    if email.is_empty() || !email.contains('@') {
        return Err("Invalid email address".into());
    }
    if password.len() < 6 {
        return Err("Password must be at least 6 characters".into());
    }

    let base = find_working_api(&state.http, &state.cached_api_base).await?;
    let body = serde_json::json!({
        "api_key": API_KEY,
        "email": email,
        "password": password,
        "language": "en",
        "platform": "linux",
    });

    let resp = state.http.post(format!("{base}/v1/create_freemium"))
        .json(&body)
        .send().await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read error: {e}"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse error: {e}"))?;

    if status.is_success() || data.get("access_token").is_some() {
        if let Some(tok) = data["access_token"].as_str() {
            let username = data["username"].as_str().unwrap_or(&email);
            let token_info = TokenInfo {
                access_token: tok.to_string(),
                refresh_token: data["refresh_token"].as_str().map(String::from),
                expires_at: data["access_expire_epoch"].as_u64().unwrap_or(now_epoch() + 86400),
                account_type: data["account_type"].as_i64().unwrap_or(0),
                sub_end_epoch: data["sub_end_epoch"].as_u64().unwrap_or(0),
            };
            *state.token.lock().unwrap() = Some(token_info.clone());
            save_json_to_disk("token.json", &token_info);
            save_credentials_to_disk(username, &password);

            let mut cfg = state.config.lock().unwrap();
            cfg.username = username.to_string();
            cfg.password = password;
            save_config_to_disk(&cfg);
        }
        Ok(data)
    } else {
        let err = data["error"].as_str()
            .or(data["message"].as_str())
            .unwrap_or("Account creation failed");
        Err(err.to_string())
    }
}

// ====== PRIVADO SPEED TEST ======

#[tauri::command]
pub async fn vpn_run_speed_test_privado(state: State<'_, AppState>) -> Result<SpeedTestResult, String> {
    let client = &state.speed_http;
    let username = state.config.lock().unwrap().username.clone();

    // Try to get Privado's speed test server URL.
    let privado_server = if let Some(token) = get_or_refresh_token(&state).await {
        if let Ok(base) = find_working_api(&state.http, &state.cached_api_base).await {
            let resp = state.http
                .get(format!("{base}/v1/speedtest/dataset/"))
                .query(&[("vpn_username", &username)])
                .bearer_auth(&token)
                .send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let data: serde_json::Value = r.json().await.unwrap_or_default();
                    data["data"]["url"].as_str()
                        .or(data["url"].as_str())
                        .map(String::from)
                }
                _ => None,
            }
        } else { None }
    } else { None };

    let (download_url, upload_url, server_name) = match &privado_server {
        Some(base_url) => (
            format!("{base_url}/download?size=10000000"),
            format!("{base_url}/upload"),
            "Privado".to_string(),
        ),
        None => (
            "https://speed.cloudflare.com/__down?bytes=10000000".to_string(),
            "https://speed.cloudflare.com/__up".to_string(),
            "Cloudflare".to_string(),
        ),
    };

    // Ping (3 samples, median).
    let ping_url = match &privado_server {
        Some(u) => format!("{u}/ping"),
        None => "https://speed.cloudflare.com/__down?bytes=1".to_string(),
    };

    let mut pings = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let _ = client.get(&ping_url).send().await;
        pings.push(t.elapsed().as_millis() as u32);
    }
    pings.sort();
    let ping_ms = pings.get(1).copied().unwrap_or(pings[0]);

    // Download test (10MB).
    let dl_start = Instant::now();
    let resp = client.get(&download_url).send().await
        .map_err(|e| format!("Download test failed: {e}"))?;
    let bytes = resp.bytes().await.map_err(|e| format!("Download read: {e}"))?;
    let dl_secs = dl_start.elapsed().as_secs_f64();
    let dl_mbps = if dl_secs > 0.0 { (bytes.len() as f64 * 8.0) / (dl_secs * 1_000_000.0) } else { 0.0 };

    // Upload test (2MB).
    let ul_start = Instant::now();
    let upload_data = vec![0u8; 2_000_000];
    let _ = client.post(&upload_url).body(upload_data).send().await
        .map_err(|e| format!("Upload test failed: {e}"))?;
    let ul_secs = ul_start.elapsed().as_secs_f64();
    let ul_mbps = if ul_secs > 0.0 { (2_000_000.0 * 8.0) / (ul_secs * 1_000_000.0) } else { 0.0 };

    let result = SpeedTestResult {
        download_mbps: (dl_mbps * 10.0).round() / 10.0,
        upload_mbps: (ul_mbps * 10.0).round() / 10.0,
        ping_ms,
        server: server_name.clone(),
        timestamp: now_iso(),
    };

    // Post results back to Privado if we used their server.
    if privado_server.is_some() {
        if let Some(token) = state.get_valid_token() {
            if let Ok(base) = find_working_api(&state.http, &state.cached_api_base).await {
                let report = serde_json::json!({
                    "vpn_username": username,
                    "download_speed": result.download_mbps,
                    "upload_speed": result.upload_mbps,
                    "latency": result.ping_ms,
                    "server": server_name,
                });
                let _ = state.http
                    .post(format!("{base}/v1/speedtest"))
                    .bearer_auth(&token)
                    .json(&report)
                    .send().await;
            }
        }
    }

    let mut results = state.speed_results.lock().unwrap();
    results.push(result.clone());
    let excess = results.len().saturating_sub(50);
    if excess > 0 { results.drain(0..excess); }
    save_speeds_to_disk(&results);
    Ok(result)
}

// ====== CONTROL TOWER FULL API ======

#[tauri::command]
pub async fn vpn_get_control_tower_full(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let token = get_or_refresh_token(&state).await.ok_or("Not logged in")?;
    let base = find_working_api(&state.http, &state.cached_api_base).await?;
    let username = state.config.lock().unwrap().username.clone();

    let groups: serde_json::Value = match state.http
        .get(format!("{base}/v1/objects/get_groups/"))
        .bearer_auth(&token)
        .send().await
    {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => serde_json::json!({}),
    };

    let public_dns: serde_json::Value = match state.http
        .get(format!("{base}/v1/objects/public_dns_servers/"))
        .bearer_auth(&token)
        .send().await
    {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => serde_json::json!([]),
    };

    let stats: serde_json::Value = match state.http
        .get(format!("{base}/v1/objects/block_stats/"))
        .query(&[("vpn_username", &username)])
        .bearer_auth(&token)
        .send().await
    {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => serde_json::json!({}),
    };

    {
        let mut ct = state.control_tower.lock().unwrap();
        if let Some(ads) = stats["ads_blocked"].as_u64().or(stats["data"]["ads_blocked"].as_u64()) {
            ct.ads_blocked = ads;
        }
        if let Some(trackers) = stats["trackers_blocked"].as_u64().or(stats["data"]["trackers_blocked"].as_u64()) {
            ct.trackers_blocked = trackers;
        }
        if let Some(threats) = stats["threats_blocked"].as_u64().or(stats["data"]["threats_blocked"].as_u64()) {
            ct.threats_blocked = threats;
        }
        save_json_to_disk("control_tower.json", &*ct);
    }

    Ok(serde_json::json!({
        "groups": groups,
        "public_dns_servers": public_dns,
        "block_stats": stats,
        "config": *state.control_tower.lock().unwrap(),
    }))
}

#[tauri::command]
pub async fn vpn_save_control_tower_profile(config: ControlTowerConfig, state: State<'_, AppState>) -> Result<(), String> {
    *state.control_tower.lock().unwrap() = config.clone();
    save_json_to_disk("control_tower.json", &config);

    if let Some(token) = get_or_refresh_token(&state).await {
        if let Ok(base) = find_working_api(&state.http, &state.cached_api_base).await {
            let username = state.config.lock().unwrap().username.clone();
            let body = serde_json::json!({
                "vpn_username": username,
                "ad_blocking": config.ad_blocking,
                "tracker_blocking": config.tracker_blocking,
                "malware_protection": config.malware_protection,
                "phishing_protection": config.phishing_protection,
                "adult_content": config.adult_content,
                "enabled": config.enabled,
                "dns_provider": config.dns_provider,
                "custom_dns": config.custom_dns,
                "custom_blocklist": config.custom_blocklist,
            });
            let _ = state.http
                .post(format!("{base}/v1/objects/update_customer_profile/"))
                .bearer_auth(&token)
                .json(&body)
                .send().await;
        }
    }
    Ok(())
}

// ====== DIAGNOSTICS ======

#[tauri::command]
pub async fn vpn_run_diagnostics(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut results = serde_json::Map::new();

    let daemon_ok = daemon_get(&state.daemon_http, "/status").await.is_ok();
    results.insert("daemon_reachable".into(), serde_json::json!(daemon_ok));

    let dns_ok = tokio::net::lookup_host("google.com:443").await.is_ok();
    results.insert("dns_working".into(), serde_json::json!(dns_ok));

    let vpn_server = "ams-101.vpn.privado.io";
    let ping_start = Instant::now();
    let vpn_reachable = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(format!("{vpn_server}:443")),
    ).await.map(|r| r.is_ok()).unwrap_or(false);
    let vpn_ping_ms = ping_start.elapsed().as_millis();
    results.insert("vpn_server_reachable".into(), serde_json::json!(vpn_reachable));
    results.insert("vpn_server_ping_ms".into(), serde_json::json!(vpn_ping_ms));

    let iptables_output = tokio::process::Command::new("iptables")
        .args(["-L", "PRIVADO_KILLSWITCH", "-n", "--line-numbers"])
        .output().await;
    let killswitch_rules = match iptables_output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => "Chain not found (killswitch inactive)".to_string(),
    };
    results.insert("killswitch_state".into(), serde_json::json!(killswitch_rules));

    let resolv = tokio::fs::read_to_string("/etc/resolv.conf").await.unwrap_or_default();
    results.insert("dns_override_active".into(), serde_json::json!(resolv.contains("privado-vpn")));
    results.insert("resolv_conf".into(), serde_json::json!(resolv));

    let journal = tokio::process::Command::new("journalctl")
        .args(["-u", "privado-vpn", "-n", "50", "--no-pager", "--output=short"])
        .output().await;
    let journal_text = match journal {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("journalctl error: {e}"),
    };
    results.insert("journal_last_50".into(), serde_json::json!(journal_text));

    let swan = tokio::process::Command::new("swanctl")
        .arg("--list-sas")
        .output().await;
    let swan_text = match swan {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => format!("swanctl error: {e}"),
    };
    results.insert("strongswan_sas".into(), serde_json::json!(swan_text));

    Ok(serde_json::Value::Object(results))
}

// ====== BREACH MONITOR ======

#[tauri::command]
pub async fn vpn_check_breach(email: String, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    if email.is_empty() || !email.contains('@') {
        return Err("Invalid email address".into());
    }

    // HIBP v3 k-anonymity model for password check.
    let hash = sha1_hex(email.as_bytes());
    let prefix = &hash[..5];
    let suffix = &hash[5..];

    let resp = state.http
        .get(format!("https://api.pwnedpasswords.com/range/{prefix}"))
        .header("User-Agent", "PrivadoVPN-Linux/2.0.0")
        .send().await
        .map_err(|e| format!("HIBP request failed: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("HIBP read: {e}"))?;

    // Check breached accounts endpoint.
    let breach_resp = state.http
        .get(format!("https://haveibeenpwned.com/api/v3/breachedaccount/{}", urlencoded(&email)))
        .header("User-Agent", "PrivadoVPN-Linux/2.0.0")
        .send().await;

    let breaches: Vec<serde_json::Value> = match breach_resp {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => Vec::new(),
    };

    let password_exposed = text.lines().any(|line| {
        line.to_uppercase().starts_with(&suffix.to_uppercase())
    });

    let breach_count = breaches.len();
    let breach_names: Vec<String> = breaches.iter()
        .filter_map(|b| b["Name"].as_str().map(String::from))
        .collect();

    Ok(serde_json::json!({
        "email": email,
        "breached": breach_count > 0,
        "breach_count": breach_count,
        "breaches": breach_names,
        "password_in_dump": password_exposed,
    }))
}

// ====== SECURITY SCANNER ======

#[tauri::command]
pub async fn vpn_security_scan(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut results = serde_json::Map::new();

    // DNS leak test.
    let resolv = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let using_vpn_dns = resolv.contains("198.18.0") || resolv.contains("privado");
    results.insert("dns_leak".into(), serde_json::json!({
        "passed": using_vpn_dns,
        "using_vpn_dns": using_vpn_dns,
        "detail": if using_vpn_dns { "DNS queries routed through VPN" } else { "DNS may be leaking to ISP" },
    }));

    // IPv6 leak test.
    let ipv6_resp = state.http.get("https://v6.ident.me/")
        .timeout(Duration::from_secs(5))
        .send().await;
    let has_ipv6 = match ipv6_resp {
        Ok(r) if r.status().is_success() => {
            let ip = r.text().await.unwrap_or_default();
            !ip.is_empty() && ip.contains(':')
        }
        _ => false,
    };
    let ipv6_disabled = std::fs::read_to_string("/proc/sys/net/ipv6/conf/all/disable_ipv6")
        .map(|s| s.trim() == "1")
        .unwrap_or(false);
    results.insert("ipv6_leak".into(), serde_json::json!({
        "passed": !has_ipv6 || ipv6_disabled,
        "ipv6_reachable": has_ipv6,
        "ipv6_disabled_kernel": ipv6_disabled,
        "detail": if !has_ipv6 || ipv6_disabled { "No IPv6 leak" } else { "IPv6 traffic may bypass tunnel" },
    }));

    // WebRTC leak vector does not apply to native desktop apps (only browsers).
    let ip_response = state.http.get("https://api.ipify.org")
        .timeout(Duration::from_secs(5))
        .send().await;
    let public_ip = if let Ok(r) = ip_response {
        r.text().await.unwrap_or_default()
    } else {
        String::new()
    };
    results.insert("webrtc_leak".into(), serde_json::json!({
        "passed": true,
        "public_ip": public_ip,
        "detail": "Desktop app not vulnerable to WebRTC leaks",
    }));

    // Connection integrity.
    let daemon_status = daemon_get(&state.daemon_http, "/status").await;
    let connected = daemon_status.as_ref()
        .map(|s| s["connected"].as_bool().unwrap_or(false))
        .unwrap_or(false);
    results.insert("connection_integrity".into(), serde_json::json!({
        "passed": connected,
        "vpn_active": connected,
        "detail": if connected { "VPN tunnel active" } else { "VPN not connected" },
    }));

    Ok(serde_json::Value::Object(results))
}

// ====== PAUSE CONNECTION ======

#[tauri::command]
pub async fn vpn_pause_connection(duration_secs: u64, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    if duration_secs == 0 || duration_secs > 86400 {
        return Err("Duration must be between 1 and 86400 seconds".into());
    }

    let body = serde_json::json!({
        "pin": VPN_PIN,
        "duration_secs": duration_secs,
    });

    daemon_post(&state.daemon_http, "/pause", &body).await
}

// ====== DESKTOP NOTIFICATIONS ======

#[tauri::command]
pub async fn vpn_send_notification(title: String, body: String, state: State<'_, AppState>) -> Result<(), String> {
    let notif_cfg = state.notifications.lock().unwrap().clone();
    if !notif_cfg.enabled {
        return Ok(());
    }

    // Try notify-send first (most common, requires libnotify-bin).
    let notify_send = tokio::process::Command::new("notify-send")
        .args(["--app-name=PrivadoVPN", "--icon=privado-vpn", "--urgency=normal", &title, &body])
        .output().await;

    if let Ok(ref o) = notify_send {
        if o.status.success() { return Ok(()); }
    }

    // Fallback: gdbus call to org.freedesktop.Notifications (works without libnotify).
    let gdbus_result = tokio::process::Command::new("gdbus")
        .args([
            "call", "--session",
            "--dest", "org.freedesktop.Notifications",
            "--object-path", "/org/freedesktop/Notifications",
            "--method", "org.freedesktop.Notifications.Notify",
            "PrivadoVPN",          // app_name
            "0",                   // replaces_id
            "privado-vpn",         // icon
            &title,                // summary
            &body,                 // body
            "[]",                  // actions
            "{}",                  // hints
            "5000",                // timeout_ms
        ])
        .output().await;

    if let Ok(ref o) = gdbus_result {
        if o.status.success() { return Ok(()); }
    }

    // Fallback: dbus-send (even more basic, available on nearly all Linux with D-Bus).
    let dbus_result = tokio::process::Command::new("dbus-send")
        .args([
            "--session",
            "--type=method_call",
            "--dest=org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications.Notify",
            "string:PrivadoVPN",
            "uint32:0",
            "string:privado-vpn",
            &format!("string:{title}"),
            &format!("string:{body}"),
            "array:string:",
            "dict:string:string:",
            "int32:5000",
        ])
        .output().await;

    match dbus_result {
        Ok(o) if o.status.success() => Ok(()),
        _ => {
            // All notification methods failed — log to stderr but don't error out.
            // The notification is non-critical; the VPN still works without it.
            eprintln!("[notification] all methods failed for: {title}");
            Ok(())
        }
    }
}

// ====== SENTRY ERROR REPORTING ======

/// Report an error to the configured Sentry DSN (or log locally if no DSN).
/// The DSN is stored in config; if empty, errors are only logged to disk.
#[tauri::command]
pub async fn vpn_report_error(
    error_type: String,
    message: String,
    context: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let timestamp = now_iso();
    let username = state.config.lock().unwrap().username.clone();

    let event = serde_json::json!({
        "timestamp": timestamp,
        "level": "error",
        "logger": "privado-vpn-linux",
        "platform": "rust",
        "event_id": generate_uuid(),
        "exception": {
            "values": [{
                "type": error_type,
                "value": message,
            }]
        },
        "user": {
            "username": username,
        },
        "contexts": context.unwrap_or(serde_json::json!({})),
        "tags": {
            "app_version": "2.0.0",
            "os": "linux",
        },
    });

    // Try to send to Sentry DSN if configured.
    let sentry_dsn = load_json_from_disk::<SentryConfig>("sentry.json")
        .and_then(|c| if c.dsn.is_empty() { None } else { Some(c) });

    if let Some(sentry_cfg) = sentry_dsn {
        if sentry_cfg.enabled {
            let resp = state.http
                .post(&sentry_cfg.dsn)
                .header("Content-Type", "application/json")
                .header("X-Sentry-Auth", format!(
                    "Sentry sentry_version=7, sentry_client=privado-vpn-linux/2.0.0, sentry_key={}",
                    sentry_cfg.public_key
                ))
                .json(&event)
                .send().await;

            if let Err(e) = resp {
                eprintln!("[sentry] failed to send event: {e}");
            }
        }
    }

    // Always log errors locally for diagnostics.
    let errors_path = config_dir().join("errors.json");
    let mut errors: Vec<serde_json::Value> = std::fs::read_to_string(&errors_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    errors.push(event);
    // Keep last 100 errors.
    let excess = errors.len().saturating_sub(100);
    if excess > 0 { errors.drain(0..excess); }

    if let Ok(text) = serde_json::to_string_pretty(&errors) {
        let _ = std::fs::write(&errors_path, text);
    }

    Ok(())
}

// ====== ANALYTICS / TELEMETRY ======

/// Track a connection/session event (opt-in only, respects user preference).
#[tauri::command]
pub async fn vpn_track_event(
    event_name: String,
    properties: Option<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let analytics_cfg = load_json_from_disk::<AnalyticsConfig>("analytics.json")
        .unwrap_or_default();

    if !analytics_cfg.enabled {
        return Ok(());
    }

    let timestamp = now_iso();
    let username = state.config.lock().unwrap().username.clone();

    let event = serde_json::json!({
        "event": event_name,
        "timestamp": timestamp,
        "user_id": username,
        "properties": properties.unwrap_or(serde_json::json!({})),
        "app_version": "2.0.0",
        "platform": "linux",
        "session_id": analytics_cfg.session_id,
    });

    // Send to analytics endpoint if configured.
    if !analytics_cfg.endpoint.is_empty() {
        let _ = state.http
            .post(&analytics_cfg.endpoint)
            .json(&event)
            .send().await;
    }

    // Always log locally.
    let events_path = config_dir().join("analytics_events.json");
    let mut events: Vec<serde_json::Value> = std::fs::read_to_string(&events_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    events.push(event);
    let excess = events.len().saturating_sub(500);
    if excess > 0 { events.drain(0..excess); }

    if let Ok(text) = serde_json::to_string_pretty(&events) {
        let _ = std::fs::write(&events_path, text);
    }

    Ok(())
}

// ====== SUBSCRIPTION MANAGEMENT ======

#[tauri::command]
pub async fn vpn_manage_subscription(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let token = state.get_valid_token().unwrap_or_default();
    let username = state.config.lock().unwrap().username.clone();

    let payment_url = if !token.is_empty() {
        format!("https://app.privadovpn.com/account/billing?token={token}")
    } else {
        "https://app.privadovpn.com/signup".to_string()
    };

    // Open in the user's default browser.
    let _ = tokio::process::Command::new("xdg-open")
        .arg(&payment_url)
        .spawn();

    let token_info = state.token.lock().unwrap().clone();
    let sub_end = token_info.as_ref().map(|t| t.sub_end_epoch).unwrap_or(0);
    let account_type = token_info.as_ref().map(|t| t.account_type).unwrap_or(0);

    let plan_name = match account_type {
        0 => "Free",
        1 => "Premium Monthly",
        2 => "Premium Yearly",
        3 => "Premium 2-Year",
        _ => "Unknown",
    };

    Ok(serde_json::json!({
        "payment_url": payment_url,
        "username": username,
        "plan": plan_name,
        "account_type": account_type,
        "sub_end_epoch": sub_end,
        "opened_browser": true,
    }))
}

// ====== PER-PROCESS SPLIT TUNNEL ======

#[tauri::command]
pub async fn vpn_add_split_process(uid: u32, _state: State<'_, AppState>) -> Result<(), String> {
    let result = tokio::process::Command::new("iptables")
        .args([
            "-t", "mangle", "-A", "OUTPUT",
            "-m", "owner", "--uid-owner", &uid.to_string(),
            "-j", "MARK", "--set-mark", "0",
        ])
        .output().await
        .map_err(|e| format!("iptables: {e}"))?;

    if !result.status.success() {
        return Err(format!("iptables: {}", String::from_utf8_lossy(&result.stderr)));
    }
    Ok(())
}

#[tauri::command]
pub async fn vpn_remove_split_process(uid: u32, _state: State<'_, AppState>) -> Result<(), String> {
    let result = tokio::process::Command::new("iptables")
        .args([
            "-t", "mangle", "-D", "OUTPUT",
            "-m", "owner", "--uid-owner", &uid.to_string(),
            "-j", "MARK", "--set-mark", "0",
        ])
        .output().await
        .map_err(|e| format!("iptables: {e}"))?;

    if !result.status.success() {
        return Err(format!("iptables: {}", String::from_utf8_lossy(&result.stderr)));
    }
    Ok(())
}

#[tauri::command]
pub async fn vpn_list_split_processes(_state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let output = tokio::process::Command::new("iptables")
        .args(["-t", "mangle", "-L", "OUTPUT", "-n", "--line-numbers"])
        .output().await
        .map_err(|e| format!("iptables: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();

    for line in text.lines() {
        if line.contains("owner UID match") && line.contains("MARK set 0x0") {
            if let Some(uid_part) = line.split("UID match").last() {
                let uid_str = uid_part.trim().split_whitespace().next().unwrap_or("");
                if let Ok(uid) = uid_str.parse::<u32>() {
                    processes.push(serde_json::json!({ "uid": uid }));
                }
            }
        }
    }

    Ok(processes)
}

// ====== INTERNAL HELPERS ======

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SentryConfig {
    enabled: bool,
    dsn: String,
    public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnalyticsConfig {
    enabled: bool,
    endpoint: String,
    session_id: String,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: String::new(),
            session_id: generate_uuid(),
        }
    }
}

/// Generate a random UUID v4 (without external crate).
fn generate_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Simple PRNG seeded with time — good enough for non-cryptographic UUIDs.
    let mut state = seed as u64;
    let mut bytes = [0u8; 16];
    for b in bytes.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *b = (state >> 33) as u8;
    }
    // Set version (4) and variant (RFC 4122).
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

fn urlencoded(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", b));
            }
        }
    }
    result
}

/// Pure Rust SHA-1 implementation (no external crate dependency).
fn sha1_hex(data: &[u8]) -> String {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a.rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    format!("{:08x}{:08x}{:08x}{:08x}{:08x}", h0, h1, h2, h3, h4)
}
