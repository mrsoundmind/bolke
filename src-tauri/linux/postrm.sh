#!/bin/sh
# Forgets Bolke's keyboard rule once Bolke is removed. Never fails the
# removal: containers and minimal systems have no udev.
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
  udevadm trigger --subsystem-match=input --action=change >/dev/null 2>&1 || true
fi
exit 0
