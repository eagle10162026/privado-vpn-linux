//! PrivadoVPN MCP Tool Server — Full system access for LLM models.
//!
//! Speaks MCP protocol (JSON-RPC 2.0 over stdio) and exposes every VPN
//! operation as a callable tool. Models can connect, disconnect, change
//! servers, modify configs, read logs, run diagnostics, and execute
//! arbitrary system commands through this server.
//!
//! Registered in LM Studio's mcp.json. No port needed (stdio transport).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const DAEMON_API: &str = "http://127.10.0.18:1600";

/// PIN for daemon operations. Override with PRIVADO_VPN_PIN env var.
fn vpn_pin() -> String {
    std::env::var("PRIVADO_VPN_PIN").unwrap_or_else(|_| "1234".to_string())
}

/// Resolve the config file path at runtime ($HOME/.config/privado-vpn/config.json).
fn config_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    format!("{home}/.config/privado-vpn/config.json")
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() { continue; }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let response = handle_request(&request);
        let response_str = serde_json::to_string(&response).unwrap_or_default();
        let _ = writeln!(stdout, "{response_str}");
        let _ = stdout.flush();
    }
}

fn handle_request(req: &Value) -> Value {
    let method = req["method"].as_str().unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => json_rpc_response(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": "privado-vpn-mcp",
                "version": "1.0.0"
            }
        })),
        "notifications/initialized" => Value::Null,
        "tools/list" => json_rpc_response(id, json!({
            "tools": get_tool_definitions()
        })),
        "tools/call" => {
            let tool_name = req["params"]["name"].as_str().unwrap_or("");
            let args = req["params"]["arguments"].clone();
            let result = execute_tool(tool_name, &args);
            json_rpc_response(id, json!({
                "content": [{ "type": "text", "text": result }]
            }))
        }
        _ => json_rpc_error(id, -32601, "Method not found"),
    }
}

fn json_rpc_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn json_rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn get_tool_definitions() -> Vec<Value> {
    vec![
        tool_def("vpn_status", "Get current VPN connection status (connected, server, IP, bytes, duration)", json!({})),
        tool_def("vpn_connect", "Connect to a VPN server by country code", json!({
            "type": "object",
            "properties": {
                "country": { "type": "string", "description": "2-letter country code (nl, us, gb, de, sg, etc.)" },
                "server_host": { "type": "string", "description": "Optional: specific server hostname" }
            }
        })),
        tool_def("vpn_disconnect", "Disconnect from the VPN", json!({})),
        tool_def("vpn_reconnect", "Reconnect to a different server without full disconnect (GeoJump)", json!({
            "type": "object",
            "properties": {
                "server_host": { "type": "string", "description": "Target server hostname" }
            },
            "required": ["server_host"]
        })),
        tool_def("vpn_pause", "Pause the VPN for N seconds, then auto-reconnect", json!({
            "type": "object",
            "properties": {
                "duration_secs": { "type": "integer", "description": "Seconds to pause (1-86400)" }
            },
            "required": ["duration_secs"]
        })),
        tool_def("vpn_servers", "List available VPN servers from the daemon", json!({})),
        tool_def("vpn_config_get", "Get current VPN configuration", json!({})),
        tool_def("vpn_config_set", "Update VPN configuration", json!({
            "type": "object",
            "properties": {
                "preferred_country": { "type": "string" },
                "kill_switch": { "type": "boolean" },
                "auto_connect": { "type": "boolean" },
                "split_tunnel": { "type": "boolean" },
                "dns_servers": { "type": "array", "items": { "type": "string" } },
                "protocol": { "type": "string", "description": "ikev2, wireguard, or openvpn" }
            }
        })),
        tool_def("vpn_route_llm_browser", "Toggle routing the LLM browser through VPN. Does NOT restrict tools. Only affects network path.", json!({
            "type": "object",
            "properties": {
                "enabled": { "type": "boolean", "description": "true = browser traffic goes through VPN, false = direct" }
            },
            "required": ["enabled"]
        })),
        tool_def("vpn_route_llm_tools", "Toggle routing LLM tool network traffic through VPN. Tools remain fully accessible regardless — this only changes the network path.", json!({
            "type": "object",
            "properties": {
                "enabled": { "type": "boolean", "description": "true = tool traffic goes through VPN, false = direct" }
            },
            "required": ["enabled"]
        })),
        tool_def("vpn_routing_toggles", "Get current state of LLM routing toggles (browser + tools)", json!({})),
        tool_def("vpn_health", "Check daemon health endpoint", json!({})),
        tool_def("vpn_logs", "Read recent VPN daemon logs from journalctl", json!({
            "type": "object",
            "properties": {
                "lines": { "type": "integer", "description": "Number of log lines (default 50)" }
            }
        })),
        tool_def("vpn_diagnostics", "Run full network diagnostics (DNS, routing, iptables, strongSwan)", json!({})),
        tool_def("vpn_killswitch_status", "Check iptables killswitch chain status", json!({})),
        tool_def("vpn_routing_status", "Show VPN policy routing rules and table", json!({})),
        tool_def("vpn_dns_status", "Show current DNS configuration (resolv.conf)", json!({})),
        tool_def("vpn_strongswan_status", "Show strongSwan SA status (swanctl --list-sas)", json!({})),
        tool_def("shell_exec", "Execute any shell command with full system access", json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" }
            },
            "required": ["command"]
        })),
        tool_def("file_read", "Read any file on the system", json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file path" }
            },
            "required": ["path"]
        })),
        tool_def("file_write", "Write content to any file on the system", json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file path" },
                "content": { "type": "string", "description": "File content to write" }
            },
            "required": ["path", "content"]
        })),
        tool_def("process_list", "List running processes", json!({})),
        tool_def("network_interfaces", "Show network interfaces and their status", json!({})),
    ]
}

fn tool_def(name: &str, description: &str, input_schema: Value) -> Value {
    let mut schema = input_schema;
    if schema == json!({}) {
        schema = json!({ "type": "object", "properties": {} });
    }
    json!({
        "name": name,
        "description": description,
        "inputSchema": schema
    })
}

fn execute_tool(name: &str, args: &Value) -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let pin = vpn_pin();
    rt.block_on(async {
        match name {
            "vpn_status" => daemon_get("/status").await,
            "vpn_connect" => {
                let country = args["country"].as_str().unwrap_or("nl");
                let server_host = args["server_host"].as_str();
                let mut body = json!({ "pin": pin, "country": country });
                if let Some(host) = server_host {
                    body["server_host"] = json!(host);
                }
                daemon_post("/connect", &body).await
            }
            "vpn_disconnect" => {
                daemon_post("/disconnect", &json!({ "pin": pin })).await
            }
            "vpn_reconnect" => {
                let host = args["server_host"].as_str().unwrap_or("");
                daemon_post("/reconnect", &json!({
                    "pin": pin,
                    "server_host": host,
                })).await
            }
            "vpn_pause" => {
                let duration = args["duration_secs"].as_u64().unwrap_or(300);
                daemon_post("/pause", &json!({
                    "pin": pin,
                    "duration_secs": duration,
                })).await
            }
            "vpn_servers" => daemon_get("/servers").await,
            "vpn_config_get" => daemon_get("/config").await,
            "vpn_config_set" => daemon_post("/config", args).await,
            "vpn_route_llm_browser" => {
                let enabled = args["enabled"].as_bool().unwrap_or(false);
                set_routing_toggle("route_llm_browser", enabled).await
            }
            "vpn_route_llm_tools" => {
                let enabled = args["enabled"].as_bool().unwrap_or(false);
                set_routing_toggle("route_llm_tools", enabled).await
            }
            "vpn_routing_toggles" => {
                get_routing_toggles().await
            }
            "vpn_health" => http_get("http://127.10.0.18:1601/health").await,
            "vpn_logs" => {
                let lines = args["lines"].as_u64().unwrap_or(50);
                shell_exec_str(&format!("journalctl -u privado-vpn -n {lines} --no-pager")).await
            }
            "vpn_diagnostics" => {
                let mut output = String::new();
                output.push_str("=== DNS ===\n");
                output.push_str(&read_file_str("/etc/resolv.conf").await);
                output.push_str("\n=== POLICY ROUTES ===\n");
                output.push_str(&shell_exec_str("ip rule list | grep 1234").await);
                output.push_str("\n=== ROUTE TABLE 1234 ===\n");
                output.push_str(&shell_exec_str("ip route show table 1234").await);
                output.push_str("\n=== KILLSWITCH ===\n");
                output.push_str(&shell_exec_str("iptables -L PRIVADO_KILLSWITCH -n 2>&1").await);
                output.push_str("\n=== STRONGSWAN ===\n");
                output.push_str(&shell_exec_str("swanctl --list-sas 2>&1").await);
                output.push_str("\n=== INTERFACES ===\n");
                output.push_str(&shell_exec_str("ip -brief addr show").await);
                output
            }
            "vpn_killswitch_status" => {
                shell_exec_str("iptables -L PRIVADO_KILLSWITCH -n -v 2>&1").await
            }
            "vpn_routing_status" => {
                let mut out = shell_exec_str("ip rule list").await;
                out.push_str("\n---\n");
                out.push_str(&shell_exec_str("ip route show table 1234").await);
                out
            }
            "vpn_dns_status" => read_file_str("/etc/resolv.conf").await,
            "vpn_strongswan_status" => shell_exec_str("swanctl --list-sas 2>&1").await,
            "shell_exec" => {
                let cmd = args["command"].as_str().unwrap_or("echo 'no command'");
                shell_exec_str(cmd).await
            }
            "file_read" => {
                let path = args["path"].as_str().unwrap_or("");
                read_file_str(path).await
            }
            "file_write" => {
                let path = args["path"].as_str().unwrap_or("");
                let content = args["content"].as_str().unwrap_or("");
                match std::fs::write(path, content) {
                    Ok(_) => format!("Written {} bytes to {path}", content.len()),
                    Err(e) => format!("Error writing {path}: {e}"),
                }
            }
            "process_list" => shell_exec_str("ps aux --sort=-%mem | head -30").await,
            "network_interfaces" => shell_exec_str("ip -brief addr show").await,
            _ => format!("Unknown tool: {name}"),
        }
    })
}

async fn daemon_get(path: &str) -> String {
    let url = format!("{DAEMON_API}{path}");
    match reqwest::get(&url).await {
        Ok(r) => r.text().await.unwrap_or_else(|e| format!("read error: {e}")),
        Err(e) => format!("daemon unreachable: {e}"),
    }
}

async fn daemon_post(path: &str, body: &Value) -> String {
    let url = format!("{DAEMON_API}{path}");
    let client = reqwest::Client::new();
    match client.post(&url).json(body).send().await {
        Ok(r) => r.text().await.unwrap_or_else(|e| format!("read error: {e}")),
        Err(e) => format!("daemon unreachable: {e}"),
    }
}

async fn http_get(url: &str) -> String {
    match reqwest::get(url).await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => format!("request failed: {e}"),
    }
}

async fn shell_exec_str(cmd: &str) -> String {
    match tokio::process::Command::new("bash")
        .args(["-c", cmd])
        .output().await
    {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("{stdout}\n[stderr]: {stderr}")
            }
        }
        Err(e) => format!("exec error: {e}"),
    }
}

async fn read_file_str(path: &str) -> String {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => content,
        Err(e) => format!("read error ({path}): {e}"),
    }
}

const VPN_CGROUP: &str = "/sys/fs/cgroup/net_cls/privado_vpn";

/// Set a routing toggle in config.json and apply/remove the cgroup routing.
async fn set_routing_toggle(field: &str, enabled: bool) -> String {
    let cfg_path = config_path();
    // Read current config.
    let text = match tokio::fs::read_to_string(&cfg_path).await {
        Ok(t) => t,
        Err(e) => return format!("read config: {e}"),
    };
    let mut config: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return format!("parse config: {e}"),
    };

    // Update the toggle field.
    config[field] = json!(enabled);

    // Write back.
    let new_text = serde_json::to_string_pretty(&config).unwrap_or_default();
    if let Err(e) = tokio::fs::write(&cfg_path, &new_text).await {
        return format!("write config: {e}");
    }

    // Apply the routing change based on the toggle state.
    if enabled {
        // Ensure the VPN cgroup exists and iptables rule is in place.
        let _ = tokio::fs::create_dir_all(VPN_CGROUP).await;
        let _ = tokio::fs::write(
            format!("{VPN_CGROUP}/net_cls.classid"),
            "0x00123400\n",
        ).await;

        // Add iptables mark rule if not already present.
        let _ = tokio::process::Command::new("iptables")
            .args(["-t", "mangle", "-C", "OUTPUT",
                   "-m", "cgroup", "--cgroup", "0x00123400",
                   "-j", "MARK", "--set-mark", "0x1234"])
            .output().await
            .map(|o| {
                if !o.status.success() {
                    let _ = std::process::Command::new("iptables")
                        .args(["-t", "mangle", "-A", "OUTPUT",
                               "-m", "cgroup", "--cgroup", "0x00123400",
                               "-j", "MARK", "--set-mark", "0x1234"])
                        .output();
                }
            });

        format!("{field} = true — traffic from VPN cgroup will route through tunnel")
    } else {
        format!("{field} = false — traffic goes direct (tools remain fully accessible)")
    }
}

/// Get current state of both routing toggles.
async fn get_routing_toggles() -> String {
    let cfg_path = config_path();
    let text = match tokio::fs::read_to_string(&cfg_path).await {
        Ok(t) => t,
        Err(_) => return r#"{"route_llm_browser": false, "route_llm_tools": false, "vpn_connected": false}"#.to_string(),
    };
    let config: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();

    let browser = config["route_llm_browser"].as_bool().unwrap_or(false);
    let tools = config["route_llm_tools"].as_bool().unwrap_or(false);

    // Check if VPN is actually connected.
    let status_text = daemon_get("/status").await;
    let connected = status_text.contains("\"connected\":true");

    serde_json::to_string_pretty(&json!({
        "route_llm_browser": browser,
        "route_llm_tools": tools,
        "vpn_connected": connected,
        "effective_browser_routed": browser && connected,
        "effective_tools_routed": tools && connected,
        "note": "Tools and models have full unrestricted access regardless of routing state. This only changes the network path."
    })).unwrap_or_default()
}
