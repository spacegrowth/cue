#!/bin/sh
# The test's key for dev, then sshd in front.
printf '%s\n' "$AUTHORIZED_KEY" >/home/dev/.ssh/authorized_keys
chown dev:dev /home/dev/.ssh/authorized_keys
chmod 600 /home/dev/.ssh/authorized_keys
exec /usr/sbin/sshd -D -e
