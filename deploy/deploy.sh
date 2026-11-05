#!/usr/bin/env bash
# Copies site/ to the VPS. Run from the repo root: bash deploy/deploy.sh
# Needs the SSH key at ~/.ssh/fushira_keys.pem (never commit it).
# Does NOT touch Caddy. See the steps printed at the end.
set -euo pipefail
KEY="${KEY:-$HOME/.ssh/fushira_keys.pem}"
HOST="${HOST:-ubuntu@50.19.248.193}"
SSH="ssh -i $KEY -o BatchMode=yes -o ConnectTimeout=15"

$SSH "$HOST" "sudo mkdir -p /var/www/barnaby && sudo chown ubuntu:ubuntu /var/www/barnaby"
scp -i "$KEY" -o BatchMode=yes -r site/. "$HOST:/var/www/barnaby/"
echo "Site copied to /var/www/barnaby."
echo "Next, once 'nslookup barnaby.valfred.world 8.8.8.8' shows 50.19.248.193:"
echo "  scp -i $KEY deploy/barnaby.caddy $HOST:/tmp/barnaby.caddy"
echo "  $SSH $HOST 'sudo cp /tmp/barnaby.caddy /etc/caddy/conf.d/barnaby.caddy && sudo caddy validate --config /etc/caddy/Caddyfile && sudo systemctl reload caddy'"
echo "Undo: $SSH $HOST 'sudo rm /etc/caddy/conf.d/barnaby.caddy && sudo systemctl reload caddy'"
