# Runbook: deploying and operating brdns

Target scale: 2–4 friends. Keep it boring.

## 1. Wildcard certificate

Identity is the SNI subdomain, so you need a real wildcard cert for
`*.dns.yourdomain.com`. Self-signed certs will not work for real clients.

Recommended: Let's Encrypt with a DNS-01 challenge (wildcard requires DNS-01).

```bash
# Example with certbot + a DNS provider plugin
certbot certonly --dns-cloudflare \
  --dns-cloudflare-credentials cloudflare.ini \
  -d 'dns.yourdomain.com' -d '*.dns.yourdomain.com'
```

Point these at brdns:

```toml
[certs]
cert_path = "/etc/letsencrypt/live/dns.yourdomain.com/fullchain.pem"
key_path  = "/etc/letsencrypt/live/dns.yourdomain.com/privkey.pem"
in_mem = false
```

## 2. DNS records

Point `dns.yourdomain.com` and the wildcard at your server:

```
dns.yourdomain.com      A   <server-ip>
*.dns.yourdomain.com    A   <server-ip>
```

## 3. Postgres

```bash
docker run -d --name brdns-pg \
  -e POSTGRES_USER=brdns -e POSTGRES_PASSWORD=<password> -e POSTGRES_DB=brdns \
  -v brdns-pg:/var/lib/postgresql/data \
  -p 127.0.0.1:5432:5432 postgres:16-alpine
```

Migrations run automatically at startup (`sqlx::migrate!`).

## 4. Configuration

Deploy `brdns.toml` and set secrets via environment variables (never commit
them):

```bash
export BRDNS__CONTROL_PLANE__DATABASE_URL="postgres://brdns:<password>@127.0.0.1:5432/brdns"
export BRDNS__CONTROL_PLANE__API_TOKEN="<long-random-token>"
```

Generate a token: `openssl rand -hex 32`.

## 5. Run

```bash
# systemd unit sketch
ExecStart=/usr/local/bin/s dot          # or `doh`
Restart=on-failure
```

DoT and DoH are separate processes today (pick one per `s` invocation); run
both behind one wildcard cert by starting `s dot` and `s doh` on their ports.

## 6. Onboarding a friend

```bash
export BRDNS_ADMIN_TOKEN=...
brdns-admin account                      # prints a 16-digit number
```

Give the friend:

- DoT: `1234567890123456.dns.yourdomain.com:853`
- DoH: `https://1234567890123456.dns.yourdomain.com/dns-query`

Then set their rules/upstreams (see README). The account number is a bearer
identifier — treat it like a password (see SECURITY.md).

## 7. Monitoring

- Prometheus scrape `127.0.0.1:9090/metrics`.
- Optional OTel collector at `BRDNS__OBSERVABILITY__OTEL_ENDPOINT`.
- `RUST_LOG=info` for structured JSON logs.

Alert on:

- `brdns_blocked_total` spikes (something's blocking more than expected)
- `brdns_query_duration_seconds` p99 rising (upstream slowness)
- `brdns_policy_accounts == 0` (control-plane/snapshot failure)
- `brdns_category_domains == 0` when blocklists are enabled (ingest failure)

## 8. Rollout checklist

- [ ] Wildcard cert issued and configured
- [ ] `dns.yourdomain.com` + wildcard A records pointing at the server
- [ ] Postgres up; `BRDNS__CONTROL_PLANE__DATABASE_URL` set
- [ ] `api_token` set (random, via env)
- [ ] `server.domain` and `server.fallback_account` set
- [ ] Blocklist enabled and first ingest completed (watch startup logs)
- [ ] `curl https://127.0.0.1:PORT/healthz` (control plane) and `/metrics` respond
- [ ] Test query through both DoT and DoH with a real account SNI

## 9. Backup / restore

Everything user-authored lives in Postgres. `pg_dump` is sufficient:

```bash
pg_dump "$BRDNS__CONTROL_PLANE__DATABASE_URL" > backup.sql
```

Blocklists re-download automatically; no need to back them up.
