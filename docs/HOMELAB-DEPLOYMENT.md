# Homelab deployment

This deployment runs Hub in a conventional Docker Compose service and uses an
existing host Nginx reverse proxy for TLS. By default, Docker publishes Hub
only on `127.0.0.1:8080`, so it cannot be reached directly from the Internet.

## Before starting

Choose a hostname, for example `hub.example.edu`. At the DNS provider, create:

- an `A` record for `hub.example.edu` pointing to the homelab's public IPv4;
- an `AAAA` record pointing to its public IPv6, only when IPv6 port 80 and 443
  are reachable too.

Forward TCP ports 80 and 443 from the router to the Nginx host and allow them
through its firewall. Do not forward port 8080.

Wait until the hostname resolves from a network outside the home LAN:

```sh
dig +short A hub.example.edu
curl -I http://hub.example.edu
```

The initial HTTP request may fail before Nginx is configured; the DNS result is
the important preflight check.

## Configure and launch

On the Docker host, clone or copy this repository, then create the secret
environment file. It is ignored by Git.

```sh
cd joveworks-hub
cp .env.example .env
openssl rand -hex 32
openssl rand -hex 32
chmod 600 .env
```

Edit `.env` and replace the two placeholder tokens with those outputs. Set the
public URL to the real hostname and retain the safe loopback Docker binding:

```dotenv
JOVEWORKS_PUBLIC_URL=https://hub.example.edu
JOVEWORKS_HOST_BIND=127.0.0.1
```

Set `JOVEWORKS_EDITOR_URL` only after the editor has a deployed HTTPS URL. If
it is not ready, delete its example line from `.env`: publication links will
safely redirect to their JSON API resources until it is configured.

Build and start the production stack:

```sh
docker compose -f compose.production.yaml up --build -d
docker compose -f compose.production.yaml ps
docker compose -f compose.production.yaml logs -f hub
```

Configure a TLS-enabled Nginx virtual host on the same machine. Substitute the
hostname and certificate paths managed by your existing Nginx setup:

```nginx
server {
    listen 80;
    listen [::]:80;
    server_name hub.example.edu;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name hub.example.edu;

    ssl_certificate     /etc/letsencrypt/live/hub.example.edu/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/hub.example.edu/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

Test and reload Nginx using your host's normal method, for example
`nginx -t && systemctl reload nginx`. Verify from an external network, such as
a phone with Wi-Fi disabled:

```sh
curl -i https://hub.example.edu/healthz
curl https://hub.example.edu/.well-known/joveworks
```

The first response must be `204`; the second must contain
`{"protocolVersion":1,"api":"/api/v1"}`. A browser should show the
certificate managed by Nginx for the hostname.

## Operations

Keep the named `joveworks-data` volume: it contains Hub's SQLite database.
Before an upgrade, take a SQLite `.backup` copy and verify restoring it on
another machine. Deploy an upgrade with:

```sh
docker compose -f compose.production.yaml up --build -d
docker compose -f compose.production.yaml logs --tail=100 hub
```

If Nginx is on a different host, set `JOVEWORKS_HOST_BIND=0.0.0.0` and firewall
port 8080 so only that Nginx host can reach it. Never expose Hub directly to
the public Internet.
