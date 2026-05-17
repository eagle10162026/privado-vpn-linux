//! PrivadoVPN RAG MCP Server — Retrieval-Augmented Generation over VPN state.
//!
//! Indexes and searches:
//! - Connection history (history.json)
//! - Daemon logs (journalctl -u privado-vpn)
//! - Configuration files (config.json, control_tower.json, etc.)
//! - Project documentation
//! - strongSwan state and configs
//! - Error logs (errors.json, analytics_events.json)
//! - System routing state (iptables, ip rule, resolv.conf)
//!
//! Uses simple TF-IDF keyword matching for retrieval — no embedding model needed.
//! MCP protocol over stdio (JSON-RPC 2.0).

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

/// Resolve config directory at runtime ($HOME/.config/privado-vpn).
fn config_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    format!("{home}/.config/privado-vpn")
}

/// Resolve project directory. Checks common locations in order.
fn project_dir() -> String {
    if let Ok(dir) = std::env::var("PRIVADO_VPN_PROJECT_DIR") {
        return dir;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let candidates = [
        format!("{home}/Desktop/privado-vpn"),
        format!("{home}/privado-vpn"),
        "/opt/privado-vpn".to_string(),
        "/usr/local/share/privado-vpn".to_string(),
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return candidate.clone();
        }
    }
    candidates[0].clone()
}

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();

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
        if response != Value::Null {
            let response_str = serde_json::to_string(&response).unwrap_or_default();
            let _ = writeln!(stdout_lock, "{response_str}");
            let _ = stdout_lock.flush();
        }
    }
}

fn handle_request(req: &Value) -> Value {
    let method = req["method"].as_str().unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => json_rpc_ok(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false }
            },
            "serverInfo": {
                "name": "privado-vpn-rag-mcp",
                "version": "1.0.0"
            }
        })),
        "notifications/initialized" => Value::Null,
        "tools/list" => json_rpc_ok(id, json!({
            "tools": get_tools()
        })),
        "tools/call" => {
            let name = req["params"]["name"].as_str().unwrap_or("");
            let args = req["params"]["arguments"].clone();
            let result = execute_tool(name, &args);
            json_rpc_ok(id, json!({
                "content": [{ "type": "text", "text": result }]
            }))
        }
        "resources/list" => json_rpc_ok(id, json!({
            "resources": get_resources()
        })),
        "resources/read" => {
            let uri = req["params"]["uri"].as_str().unwrap_or("");
            let content = read_resource(uri);
            json_rpc_ok(id, json!({
                "contents": [{ "uri": uri, "mimeType": "text/plain", "text": content }]
            }))
        }
        _ => json_rpc_err(id, -32601, "Method not found"),
    }
}

fn json_rpc_ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn json_rpc_err(id: Value, code: i32, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

fn get_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "vpn_rag_search",
            "description": "Search across all VPN state (logs, config, history, docs) using keyword matching. Returns the most relevant chunks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query (keywords)" },
                    "max_results": { "type": "integer", "description": "Max results to return (default 10)" },
                    "sources": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional: limit to specific sources (logs, config, history, docs, errors, routing)"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "vpn_rag_get_context",
            "description": "Get full context from a specific VPN data source",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "enum": ["config", "history", "logs", "errors", "analytics", "control_tower", "docs", "routing", "strongswan"],
                        "description": "Which data source to retrieve"
                    }
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "vpn_rag_connection_history",
            "description": "Get connection history with optional filtering",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "country": { "type": "string", "description": "Filter by country code" },
                    "limit": { "type": "integer", "description": "Max records (default all)" }
                }
            }
        }),
        json!({
            "name": "vpn_rag_error_log",
            "description": "Get recent errors and their context",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max errors (default 20)" }
                }
            }
        }),
    ]
}

fn get_resources() -> Vec<Value> {
    vec![
        json!({ "uri": "privado://config", "name": "VPN Configuration", "mimeType": "application/json" }),
        json!({ "uri": "privado://history", "name": "Connection History", "mimeType": "application/json" }),
        json!({ "uri": "privado://docs", "name": "Project Documentation", "mimeType": "text/markdown" }),
        json!({ "uri": "privado://logs", "name": "Daemon Logs (last 100 lines)", "mimeType": "text/plain" }),
        json!({ "uri": "privado://errors", "name": "Error Log", "mimeType": "application/json" }),
        json!({ "uri": "privado://routing", "name": "Routing State", "mimeType": "text/plain" }),
        json!({ "uri": "privado://strongswan", "name": "strongSwan Config", "mimeType": "text/plain" }),
        json!({ "uri": "privado://control_tower", "name": "Control Tower Config", "mimeType": "application/json" }),
    ]
}

fn read_resource(uri: &str) -> String {
    let cfg = config_dir();
    let proj = project_dir();
    match uri {
        "privado://config" => read_file(&format!("{cfg}/config.json")),
        "privado://history" => read_file(&format!("{cfg}/history.json")),
        "privado://docs" => read_file(&format!("{proj}/README.md")),
        "privado://logs" => shell_sync("journalctl -u privado-vpn -n 100 --no-pager 2>&1"),
        "privado://errors" => read_file(&format!("{cfg}/errors.json")),
        "privado://routing" => {
            let mut out = shell_sync("ip rule list");
            out.push_str("\n---\n");
            out.push_str(&shell_sync("ip route show table 1234 2>&1"));
            out.push_str("\n---\n");
            out.push_str(&shell_sync("iptables -L PRIVADO_KILLSWITCH -n 2>&1"));
            out
        }
        "privado://strongswan" => {
            let mut out = shell_sync("swanctl --list-sas 2>&1");
            out.push_str("\n---\n");
            out.push_str(&read_file("/etc/swanctl/conf.d/privado.conf"));
            out
        }
        "privado://control_tower" => read_file(&format!("{cfg}/control_tower.json")),
        _ => format!("Unknown resource: {uri}"),
    }
}

fn execute_tool(name: &str, args: &Value) -> String {
    match name {
        "vpn_rag_search" => {
            let query = args["query"].as_str().unwrap_or("");
            let max_results = args["max_results"].as_u64().unwrap_or(10) as usize;
            let sources: Vec<&str> = args["sources"].as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_else(|| vec!["logs", "config", "history", "docs", "errors", "routing"]);
            rag_search(query, max_results, &sources)
        }
        "vpn_rag_get_context" => {
            let source = args["source"].as_str().unwrap_or("config");
            get_full_context(source)
        }
        "vpn_rag_connection_history" => {
            let country = args["country"].as_str();
            let limit = args["limit"].as_u64();
            get_connection_history(country, limit)
        }
        "vpn_rag_error_log" => {
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            get_error_log(limit)
        }
        _ => format!("Unknown tool: {name}"),
    }
}

/// Simple keyword-based RAG search across all indexed sources.
fn rag_search(query: &str, max_results: usize, sources: &[&str]) -> String {
    let keywords: Vec<&str> = query.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() >= 2)
        .collect();

    if keywords.is_empty() {
        return "No valid search keywords provided".to_string();
    }

    let mut chunks: Vec<(f64, String, String)> = Vec::new();

    for source in sources {
        let content = get_full_context(source);
        let source_chunks = chunk_text(&content, 500);

        for chunk in source_chunks {
            let score = score_chunk(&chunk, &keywords);
            if score > 0.0 {
                chunks.push((score, source.to_string(), chunk));
            }
        }
    }

    chunks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    chunks.truncate(max_results);

    if chunks.is_empty() {
        return format!("No results found for query: {query}");
    }

    let mut output = format!("Found {} results for \"{query}\":\n\n", chunks.len());
    for (i, (score, source, text)) in chunks.iter().enumerate() {
        output.push_str(&format!("--- Result {} [source: {source}, score: {score:.2}] ---\n", i + 1));
        output.push_str(text);
        output.push_str("\n\n");
    }
    output
}

/// Score a chunk of text against search keywords using TF-IDF-like scoring.
fn score_chunk(chunk: &str, keywords: &[&str]) -> f64 {
    let chunk_lower = chunk.to_lowercase();
    let chunk_words: Vec<&str> = chunk_lower.split_whitespace().collect();
    let total_words = chunk_words.len() as f64;
    if total_words == 0.0 { return 0.0; }

    let mut score = 0.0;
    for keyword in keywords {
        let kw_lower = keyword.to_lowercase();
        let count = chunk_words.iter().filter(|w| w.contains(&kw_lower.as_str())).count() as f64;
        let tf = count / total_words;
        let exact_bonus = if chunk_lower.contains(&kw_lower) { 1.0 } else { 0.0 };
        score += tf * 10.0 + exact_bonus;
    }
    score
}

/// Split text into overlapping chunks of approximately `size` characters.
fn chunk_text(text: &str, size: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in lines {
        if current.len() + line.len() > size && !current.is_empty() {
            chunks.push(current.clone());
            let overlap: String = current.lines().rev().take(2).collect::<Vec<_>>()
                .into_iter().rev().collect::<Vec<_>>().join("\n");
            current = overlap;
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

fn get_full_context(source: &str) -> String {
    let cfg = config_dir();
    let proj = project_dir();
    match source {
        "config" => {
            let mut out = read_file(&format!("{cfg}/config.json"));
            out.push_str("\n---credentials---\n[REDACTED]\n");
            out.push_str("\n---token---\n");
            out.push_str(&read_file(&format!("{cfg}/token.json")));
            out
        }
        "history" => read_file(&format!("{cfg}/history.json")),
        "logs" => shell_sync("journalctl -u privado-vpn -n 200 --no-pager 2>&1"),
        "errors" => read_file(&format!("{cfg}/errors.json")),
        "analytics" => read_file(&format!("{cfg}/analytics_events.json")),
        "control_tower" => read_file(&format!("{cfg}/control_tower.json")),
        "docs" => read_file(&format!("{proj}/README.md")),
        "routing" => {
            let mut out = String::from("=== resolv.conf ===\n");
            out.push_str(&read_file("/etc/resolv.conf"));
            out.push_str("\n=== ip rule ===\n");
            out.push_str(&shell_sync("ip rule list"));
            out.push_str("\n=== table 1234 ===\n");
            out.push_str(&shell_sync("ip route show table 1234 2>&1"));
            out.push_str("\n=== killswitch ===\n");
            out.push_str(&shell_sync("iptables -L PRIVADO_KILLSWITCH -n -v 2>&1"));
            out.push_str("\n=== mangle OUTPUT ===\n");
            out.push_str(&shell_sync("iptables -t mangle -L OUTPUT -n 2>&1"));
            out
        }
        "strongswan" => {
            let mut out = String::from("=== swanctl --list-sas ===\n");
            out.push_str(&shell_sync("swanctl --list-sas 2>&1"));
            out.push_str("\n=== privado.conf ===\n");
            out.push_str(&read_file("/etc/swanctl/conf.d/privado.conf"));
            out.push_str("\n=== privado-secrets.conf ===\n[REDACTED - secrets file]\n");
            out.push_str("\n=== systemctl status strongswan ===\n");
            out.push_str(&shell_sync("systemctl status strongswan --no-pager 2>&1"));
            out
        }
        _ => format!("Unknown source: {source}"),
    }
}

fn get_connection_history(country: Option<&str>, limit: Option<u64>) -> String {
    let cfg = config_dir();
    let text = read_file(&format!("{cfg}/history.json"));
    if text.starts_with("read error") || text.is_empty() {
        return "No connection history found".to_string();
    }

    let records: Vec<Value> = serde_json::from_str(&text).unwrap_or_default();
    let filtered: Vec<&Value> = records.iter()
        .filter(|r| {
            if let Some(cc) = country {
                r["country_code"].as_str().map(|c| c.eq_ignore_ascii_case(cc)).unwrap_or(false)
            } else {
                true
            }
        })
        .collect();

    let limited: Vec<&Value> = match limit {
        Some(n) => filtered.into_iter().rev().take(n as usize).collect(),
        None => filtered,
    };

    serde_json::to_string_pretty(&limited).unwrap_or_else(|_| "Parse error".to_string())
}

fn get_error_log(limit: usize) -> String {
    let cfg = config_dir();
    let text = read_file(&format!("{cfg}/errors.json"));
    if text.starts_with("read error") || text.is_empty() {
        return "No errors logged".to_string();
    }

    let errors: Vec<Value> = serde_json::from_str(&text).unwrap_or_default();
    let recent: Vec<&Value> = errors.iter().rev().take(limit).collect();
    serde_json::to_string_pretty(&recent).unwrap_or_else(|_| "Parse error".to_string())
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| format!("read error ({path}): {e}"))
}

fn shell_sync(cmd: &str) -> String {
    match std::process::Command::new("bash")
        .args(["-c", cmd])
        .output()
    {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout);
            let err = String::from_utf8_lossy(&o.stderr);
            if err.is_empty() { out.to_string() } else { format!("{out}\n[stderr]: {err}") }
        }
        Err(e) => format!("exec error: {e}"),
    }
}
