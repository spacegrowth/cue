#!/bin/sh
# Turns a plain Ubuntu container into a machine for Cue's machine tests (scripts/test-machines.sh saves the result as
# the image): sshd, nc (the tests reach sshd through `docker exec … nc`, so no port is opened), and a user, dev,
# whose key the test gives at start (start.sh). ~/bin is on dev's login PATH (Ubuntu's .profile): the tests put
# their stand-in agent there.
set -e
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends openssh-server procps netcat-openbsd >/dev/null
rm -rf /var/lib/apt/lists/*
mkdir -p /run/sshd
useradd -m -s /bin/bash dev
mkdir -p /home/dev/.ssh /home/dev/bin
chown -R dev:dev /home/dev
cp /setup/start.sh /start.sh
chmod 755 /start.sh
