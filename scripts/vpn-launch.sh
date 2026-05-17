#!/bin/bash
# Launch any application with its traffic routed through the PrivadoVPN tunnel.
#
# Usage:
#   vpn-launch <command> [args...]
#   vpn-launch stygian-mcp
#   vpn-launch firefox --private-window
#   vpn-launch curl https://ipinfo.io
#
# How it works:
#   Places the process in the net_cls cgroup "privado_vpn" which marks all
#   its outbound packets with classid 0x00123400. The daemon's iptables rule
#   converts this to fwmark 0x1234, which strongSwan's XFRM policy matches
#   and encrypts through the IPsec tunnel.
#
# Everything NOT launched with this script goes direct (no VPN).

VPN_CGROUP="/sys/fs/cgroup/net_cls/privado_vpn"

if [ $# -eq 0 ]; then
    echo "Usage: vpn-launch <command> [args...]"
    echo ""
    echo "Routes the given command's traffic through the PrivadoVPN tunnel."
    echo "Everything else on the system stays on direct internet."
    exit 1
fi

# Ensure the cgroup exists (the daemon creates it on connect, but handle the
# case where it's called before the daemon has set it up).
if [ ! -d "$VPN_CGROUP" ]; then
    sudo mkdir -p "$VPN_CGROUP"
    echo "0x00123400" | sudo tee "$VPN_CGROUP/net_cls.classid" > /dev/null
fi

# Check if the VPN daemon is actually connected.
VPN_STATUS=$(curl -s --connect-timeout 2 http://127.10.0.18:1600/status 2>/dev/null)
CONNECTED=$(echo "$VPN_STATUS" | grep -o '"connected":true')
if [ -z "$CONNECTED" ]; then
    echo "[vpn-launch] WARNING: VPN is not connected. Traffic will go direct."
    echo "[vpn-launch] Connect first with: privado-vpn connect"
fi

# Launch the command inside the VPN cgroup.
# cgexec places the process (and all children) in the specified cgroup.
exec cgexec -g net_cls:privado_vpn "$@"
