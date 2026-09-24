#!/bin/sh
# command-chain launcher: picks the ALSA "default" route for the host's sound
# server before starting the app. PipeWire hosts get the native pipewire
# plugin (asound-pipewire.conf), anything else the pulse plugin (asound.conf).
# alsa-snap.conf comes first in both: the staged alsa.conf without the hooks
# that load host ALSA config, which would otherwise override "default".
if timeout 3 pactl info 2>/dev/null | grep -q "on PipeWire"; then
    route="$SNAP/etc/asound-pipewire.conf"
else
    route="$SNAP/etc/asound.conf"
fi
export ALSA_CONFIG_PATH="$SNAP/etc/alsa-snap.conf:$route"
exec "$@"
