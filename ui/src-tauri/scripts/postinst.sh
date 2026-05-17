#!/bin/bash
set -e

# Set setuid bit on the privileged helper binary
# Required for IKEv2/OpenVPN/WireGuard tunnel management, iptables kill switch,
# DNS configuration, and per-app bypass rules.
if [ -f /usr/sbin/privado-vpn-helper ]; then
    chmod u+s /usr/sbin/privado-vpn-helper
fi

# Ensure strongSwan charon daemon is enabled and running
if command -v systemctl >/dev/null 2>&1; then
    systemctl enable strongswan 2>/dev/null || true
    systemctl start strongswan 2>/dev/null || true
fi

# Update icon cache
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi

# Update desktop database
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications 2>/dev/null || true
fi

exit 0
