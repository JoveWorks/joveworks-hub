# Deploy JoveWorks Hub on this server

The prepared production setup exposes Hub as
`https://jovehub.thomasvanriel.com`. Docker listens only on
`127.0.0.1:8083`; the host Nginx instance is the only public entry point.
Port 8083 was selected because this server's port 8080 is already occupied.

All paths below assume this repository is the current directory. The files in
`deploy/nginx/` are templates to copy into the host configuration; deployment
commands that use `sudo` intentionally modify the host and must be run by the
server administrator.

## 1. DNS and router port forwarding

Create an `A` record:

```text
jovehub.thomasvanriel.com  ->  <the server's public IPv4 address>
```

This server manages Netlify DNS through `~/nginx/update-dns.sh`. One option is
to add `jovehub` to the `subdomains` list in `~/nginx/dns-config.yaml`, then
follow that script's help/output. Alternatively create the record in Netlify's
DNS dashboard. Add an `AAAA` record only if IPv6 is intentionally configured
and ports 80 and 443 are reachable over IPv6.

On the Internet router, forward TCP **80** and **443** to this Nginx server.
Allow TCP 80/443 through the server firewall if it is enabled. Do **not**
forward 8083: it is loopback-only and is solely for Nginx-to-container traffic.

Confirm public DNS before requesting a certificate:

```sh
dig +short A jovehub.thomasvanriel.com
```

It must return the same public IP that accepts the port forwards. Certificate
issuance will fail until Let's Encrypt can reach port 80 from the Internet.

## 2. Create secrets and start the container

Create the ignored environment file:

```sh
cp .env.example .env
openssl rand -hex 32
openssl rand -hex 32
chmod 600 .env
```

Put the first random value in `JOVEWORKS_ADMIN_TOKEN` and the second in
`JOVEWORKS_COURSE_TOKEN`. Do not commit or publish `.env`. The prepared public
URL and host binding should remain:

```dotenv
JOVEWORKS_PUBLIC_URL=https://jovehub.thomasvanriel.com
JOVEWORKS_HOST_BIND=127.0.0.1
JOVEWORKS_HOST_PORT=8083
```

`JOVEWORKS_EDITOR_URL` must be the real HTTPS origin of the JoveWorks editor.
If the editor is not deployed yet, remove or comment out that line; Hub will
still serve its API, but editor-backed share links will not work yet.

Build and start the service:

```sh
docker compose -f compose.production.yaml up --build -d
docker compose -f compose.production.yaml ps
docker compose -f compose.production.yaml logs --tail=100 hub
curl -i http://127.0.0.1:8083/healthz
```

The last command must return `HTTP/1.1 204 No Content`. Docker persists the
SQLite database in a named volume whose name ends in `joveworks-data`.

## 3. Bootstrap Nginx over HTTP

The repository includes an HTTP-only bootstrap host because Nginx cannot load
certificate paths before the first certificate exists:

```sh
sudo cp deploy/nginx/jovehub.bootstrap.conf /etc/nginx/sites-available/jovehub.conf
sudo ln -s /etc/nginx/sites-available/jovehub.conf /etc/nginx/sites-enabled/jovehub.conf
sudo nginx -t
sudo systemctl reload nginx
curl -i http://jovehub.thomasvanriel.com/healthz
```

If the symlink already exists, leave it in place. The public curl should now
return 204. If it does not, resolve DNS, router, firewall, or upstream issues
before continuing.

## 4. Issue the Let's Encrypt certificate with Certbot

Install the Nginx plugin if it is not already present (package names shown are
for Debian/Ubuntu), then request the certificate:

```sh
sudo apt update
sudo apt install certbot python3-certbot-nginx
sudo certbot certonly --nginx -d jovehub.thomasvanriel.com
```

Supply the operational email address requested by Certbot and accept the terms.
Certbot stores the certificate beneath
`/etc/letsencrypt/live/jovehub.thomasvanriel.com/`.

Now install the final HTTPS virtual host that matches the conventions in
`~/nginx/sites-available`:

```sh
sudo cp deploy/nginx/jovehub.conf /etc/nginx/sites-available/jovehub.conf
sudo nginx -t
sudo systemctl reload nginx
```

Test automatic renewal and reload behavior:

```sh
sudo certbot renew --dry-run
systemctl status certbot.timer
```

On installations without an active Certbot timer, enable it with
`sudo systemctl enable --now certbot.timer`.

## 5. End-to-end verification

Run these checks from another network (for example, a phone with Wi-Fi off):

```sh
curl -i https://jovehub.thomasvanriel.com/healthz
curl https://jovehub.thomasvanriel.com/.well-known/joveworks
```

The health endpoint must return 204. Discovery must return JSON containing
`"protocolVersion":1` and `"api":"/api/v1"`. Also check that HTTP redirects:

```sh
curl -I http://jovehub.thomasvanriel.com/healthz
```

It should return a 301 with an HTTPS `Location`. Browser certificate details
should name `jovehub.thomasvanriel.com` and show no trust warning.

## Operations and upgrades

View health and logs:

```sh
docker compose -f compose.production.yaml ps
docker compose -f compose.production.yaml logs -f hub
```

Rebuild and deploy a repository update:

```sh
docker compose -f compose.production.yaml build --pull
docker compose -f compose.production.yaml up -d
docker compose -f compose.production.yaml logs --tail=100 hub
```

Back up the SQLite database before upgrades. Locate the volume first:

```sh
docker volume ls --filter name=joveworks-data
```

Use SQLite's `.backup` command from a temporary maintenance container or the
documented application backup process; do not copy a live SQLite file directly.
Never remove the named volume with `docker compose down -v` unless permanent
database deletion is intended.

To stop the application without deleting its database:

```sh
docker compose -f compose.production.yaml down
```

If Nginx reports `502 Bad Gateway`, check `docker compose ... ps`, container
logs, and `curl http://127.0.0.1:8083/healthz`. If TLS issuance fails, confirm
public DNS and inbound port 80 before retrying Certbot.
