#!/bin/sh
# Uninstalls logitech-sim-led.
#
# macOS .pkg installers don't register with any OS-level "uninstall"
# mechanism the way Windows MSIs or Linux .debs do -- Apple's Installer.app
# only tracks *receipts* (what a pkg installed, for `pkgutil --forget`
# bookkeeping), not a one-click remove. This script is installed alongside
# the binaries specifically so it's still around later, without needing to
# keep the original .pkg download -- see docs/user-guide.md.
set -e

echo "Removing logitech-sim-led..."
sudo rm -f /usr/local/bin/sld-service /usr/local/bin/sld-cli
sudo rm -rf /usr/local/etc/logitech-sim-led
sudo pkgutil --forget com.gschuurman.logitech-sim-led >/dev/null 2>&1 || true
echo "Done. (This script and its containing folder, /usr/local/share/logitech-sim-led, can be removed manually too.)"
