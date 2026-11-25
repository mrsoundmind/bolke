#!/bin/sh
# Applies Bolke's keyboard rule now, so the shortcut works without logging
# out. Never fails the install: containers and minimal systems have no udev.
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
  udevadm trigger --subsystem-match=input --action=change >/dev/null 2>&1 || true
fi
exit 0
