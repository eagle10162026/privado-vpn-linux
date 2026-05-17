# PrivadoVPN for Linux

**Unofficial, open-source Linux client for PrivadoVPN.**

Built via interoperability reverse engineering of the official Android APK (legal under DMCA Section 1201(f) and EU Directive 2009/24/EC Article 6). This project is NOT affiliated with, endorsed by, or associated with Privado Networks AG or any of its subsidiaries.

## Status

**Code complete. NOT yet live-tested.** All components compile and install. VPN connection has not yet been verified against live servers. Use at your own risk.

## Requirements

- A valid PrivadoVPN subscription (free or paid)
- Linux (tested on Debian/Ubuntu-based systems with kernel 5.x+)
- strongSwan 5.9+ (for IKEv2)
- WireGuard tools (`wg`, `wg-quick`) — optional, for WireGuard protocol
- OpenVPN 2.5+ — optional, for OpenVPN protocol
- Rust 1.75+ and Bun 1.0+ (for building)
- Root access (the daemon manages network routes and iptables rules)

## Features

| # | Feature | Description |
|---|---------|-------------|
| 1 | IKEv2 via strongSwan | Full IPsec tunnel with automatic config generation |
| 2 | WireGuard protocol | Native WireGuard via wg-quick with key exchange |
| 3 | OpenVPN protocol | UDP with TCP fallback and XOR scramble detection |
| 4 | Auto protocol switching | IKEv2 → WG → OpenVPN fallback chain |
| 5 | GeoJump | Atomic server switch without full disconnect |
| 6 | Ping-based server selection | TCP RTT measurement to pick fastest server |
| 7 | Kill switch | iptables chain blocks non-VPN traffic on disconnect |
| 8 | Split tunneling | Per-domain routing via policy routes |
| 9 | Per-process VPN routing | cgroup net_cls marks — only opted-in apps use VPN |
| 10 | Auto-connect on boot | Config-guarded, never connects without user opt-in |
| 11 | Trusted networks | Auto-disconnect on known WiFi SSIDs |
| 12 | Pause connection | Timed disconnect with automatic resume |
| 13 | DNS leak protection | resolv.conf override with Privado's DNS servers |
| 14 | Speed test | Privado speed servers with Cloudflare fallback |
| 15 | Breach monitor | HIBP k-anonymity check (pure Rust SHA-1) |
| 16 | Security scanner | DNS leak, IPv6 leak, WebRTC, connection integrity |
| 17 | Desktop notifications | notify-send → gdbus → dbus-send fallback chain |
| 18 | Token refresh | OAuth refresh_token first, re-login fallback |
| 19 | Control Tower API | Full sync with Privado's server management API |
| 20 | Diagnostics | Daemon, DNS, iptables, routing, journalctl, strongSwan |
| 21 | Account creation | Freemium account signup from the client |
| 22 | MCP tool server | 21 tools for LLM integration (connect, disconnect, diagnostics) |
| 23 | MCP RAG server | Keyword search over logs, config, and connection history |
| 24 | Tauri desktop UI | Native GTK app with SvelteKit frontend |

## Architecture

```
┌─────────────────────────────────────────────────┐
│  Tauri Desktop App (SvelteKit + Rust)           │
│  - Login, server picker, settings, speed test   │
└─────────────────────┬───────────────────────────┘
                      │ HTTP API (127.10.0.18:1600)
┌─────────────────────▼───────────────────────────┐
│  privado-vpn daemon (runs as root)              │
│  - strongSwan/WG/OpenVPN management             │
│  - Policy routing (fwmark + ip rule)            │
│  - Kill switch (iptables)                       │
│  - Token management + Portal API calls          │
└─────────────────────────────────────────────────┘
```

**Routing model:** Default = everything goes direct (no VPN). Only processes explicitly placed in the `privado_vpn` cgroup have their traffic routed through the tunnel. This prevents the VPN from breaking local services, LAN access, or other applications.

## Building

```bash
# Clone
git clone https://github.com/eagle10162026/privado-vpn-linux.git
cd privado-vpn-linux

# Build the daemon
cargo build --release
sudo cp target/release/privado-vpn /usr/local/bin/

# Install systemd service
sudo cp systemd/privado-vpn.service /etc/systemd/system/
sudo mkdir -p /etc/systemd/system/privado-vpn.service.d/
sudo cp systemd/restart-limits.conf /etc/systemd/system/privado-vpn.service.d/
sudo systemctl daemon-reload
sudo systemctl enable --now privado-vpn

# Build the Tauri desktop UI
cd ui
bun install
bun run build
cd src-tauri && cargo tauri build
sudo dpkg -i target/release/bundle/deb/PrivadoVPN_*.deb

# Build MCP servers (optional, for LLM integration)
cd ../../mcp/privado-vpn-mcp && cargo build --release
cd ../privado-vpn-rag-mcp && cargo build --release
```

## Usage

```bash
# Log in
privado-vpn login

# Connect to a country
privado-vpn connect nl

# Check status
privado-vpn status

# Disconnect
privado-vpn disconnect

# Route a specific app through VPN
scripts/vpn-launch.sh firefox --private-window
scripts/vpn-launch.sh curl https://ipinfo.io
```

## Configuration

Config lives at `~/.config/privado-vpn/config.json`. The daemon PIN defaults to `1234` and can be overridden via `PRIVADO_VPN_PIN` environment variable.

Key settings:
- `preferred_country` — default server country code
- `protocol` — "ikev2", "wireguard", or "openvpn"
- `kill_switch` — block traffic if VPN drops (default: true)
- `auto_connect` — connect on daemon start (default: false)
- `trusted_networks` — WiFi SSIDs where VPN auto-disconnects
- `split_tunnel` / `split_domains` — per-domain routing
- `route_llm_browser` / `route_llm_tools` — LLM traffic routing toggles

## Legal Basis

This software was created through **interoperability reverse engineering** of the PrivadoVPN Android application, which is lawful under:

- **DMCA Section 1201(f)** (United States): Permits reverse engineering for the purpose of achieving interoperability with independently created programs.
- **EU Directive 2009/24/EC, Article 6** (European Union): Permits decompilation where necessary to achieve interoperability, provided the information is not otherwise readily available.
- **Computer Programs Directive** (EU): Explicitly protects interface information obtained through legitimate reverse engineering.

No proprietary source code was copied. The API endpoints, authentication flows, and protocol parameters were observed through network traffic analysis and APK decompilation (public interface observation). The implementation is entirely original code.

## License

**GNU Affero General Public License v3.0 (AGPL-3.0)**

You are free to use, modify, and distribute this software. If you modify it and run it as a network service, or distribute it in any form, you MUST release your complete source code under the same license.

This effectively means:
- Individuals and communities can freely use and modify this software
- Corporations cannot take this code, close the source, and sell it as a proprietary product
- Any derivative work must remain open source under AGPL-3.0

See [LICENSE](LICENSE) for the full text.

## Disclaimer

THIS SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND. This is an unofficial third-party client. PrivadoVPN may change their API at any time, which could break this client. The authors are not responsible for any consequences of using this software, including but not limited to account termination, data loss, or privacy breaches.

Not affiliated with Privado Networks AG.
