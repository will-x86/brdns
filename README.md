# brdns

Multi-tenant encrypted DNS filtering service in Rust.

`brdns` is a DoT/DoH proxy that identifies each **account** by the SNI
subdomain the client dials, and applies per-account policy: ordered
allow/deny/limit rules, per-category quotas, community blocklists, and custom
upstreams. No email, no PII — an account is just an opaque number.

Built on [pingora](https://github.com/cloudflare/pingora), with an axum +
Postgres control plane behind a swappable trait.

## Components

| Binary         | Purpose                                                                 |
| -------------- | ----------------------------------------------------------------------- |
| `s`            | DoT (853) / DoH (443) server: terminates TLS, identifies accounts, enforces policy |
| `brdns-admin`  | CLI for managing accounts, rules, and upstreams via the control-plane API |
| `g`            | Query tool (UDP / DoT / DoH) for testing                                 |

## How identity works

Both DoT and DoH read the TLS **SNI**: the account is the leftmost label of
`{account}.dns.yourdomain.com`.

- DoH endpoint: `https://{account}.dns.yourdomain.com/dns-query`
- DoT endpoint: `{account}.dns.yourdomain.com:853`

No URL tokens, no mTLS, no login. You need a real wildcard certificate for
`*.dns.yourdomain.com` (see [RUNBOOK.md](RUNBOOK.md)).

## Quick start (local, in-memory)

```bash
# DoT server (defaults: 8853 -> 1.1.1.1:853)
cargo run --bin s dot

# DoH server (defaults: 6188 -> cloudflare-dns.com)
cargo run --bin s doh

# Query via DoH
cargo run --bin g doh -d google.com
```

With no Postgres configured, an in-memory control plane is used, so the server
runs with zero infrastructure.

## Management

Enable the control plane and set a token:

```toml
[control_plane]
enabled = true
listen_addr = "127.0.0.1:8080"
api_token = "your-secret-token"   # or BRDNS__CONTROL_PLANE__API_TOKEN
```

Then manage policy with `brdns-admin`:

```bash
export BRDNS_ADMIN_TOKEN=your-secret-token

# Create an account (prints a 16-digit number)
brdns-admin account

# Block ads by category
brdns-admin rules 1234567890123456 add --action deny --target category ads

# Limit YouTube to 10,000 queries/month
brdns-admin rules 1234567890123456 add --action limit --target category youtube \
    --limit 10000 --window month

# Allow a site explicitly (first match wins)
brdns-admin rules 1234567890123456 add --action allow --target domain example.com

# Replace rules from a JSON file
brdns-admin rules 1234567890123456 set rules.json

# Use a preset or custom upstream
brdns-admin presets
brdns-admin upstreams 1234567890123456 set upstreams.json
```

Rules are evaluated in order, first match wins (Pi-hole style):

- `domain`  — exact match
- `wildcard` — `*.example.com` (matches the domain and subdomains)
- `category` — membership in a blocklist category (e.g. `ads`, `tracking`)

`limit` rules count queries per window (`hour|day|week|month`).

## Configuration

All keys optional; `brdns.toml` or `BRDNS_*` env vars. See `brdns.toml` for the
full annotated example.

```toml
[server]
domain = "dns.example.com"      # base domain for SNI identity
fallback_account = "default"    # used for unknown SNI; "" refuses

[dot]
listen_port = 8853
upstream_host = "1.1.1.1"       # default upstream (fallback)
upstream_port = 853

[doh]
listen_port = 6188
upstream_host = "cloudflare-dns.com"
upstream_addr = "1.1.1.1:443"   # pinned IP

[control_plane]
enabled = true
listen_addr = "127.0.0.1:8080"
database_url = "postgres://brdns:pass@127.0.0.1:5432/brdns"
api_token = "secret"
policy_refresh_secs = 30        # rules/upstreams poll interval

[blocklist]
enabled = true                  # fetch community feeds at startup
refresh_interval_secs = 86400   # and re-fetch daily

[policy]
block_response = "nxdomain"     # nxdomain | refused | null | custom
custom_ipv4 = "0.0.0.0"
custom_ipv6 = "::"

[observability]
metrics_addr = "127.0.0.1:9090" # Prometheus /metrics
otel_endpoint = "http://localhost:4318/v1/traces"  # optional OTLP traces
```

## Observability

- **Prometheus** `/metrics`: queries by account/outcome/protocol, latency
  histograms, blocked counters, and gauge sizes. Aggregate only.
- **OpenTelemetry** traces (OTLP HTTP) with `account` + `protocol` attributes.
- **Structured JSON logs** (set `RUST_LOG=info`).

No query names ever appear in logs, metrics, or traces. See
[PRIVACY.md](PRIVACY.md).

## Blocklists

Built-in, license-reviewed feeds: OISD, StevenBlack, Hagezi, AdGuard DNS — all
tagged `ads` (~350k unique domains). Add your own feeds (any category) via
`[blocklist.feeds]`:

```toml
[[blocklist.feeds]]
name = "my-gambling-list"
url = "https://example.com/list.txt"
format = "plain"          # plain | hosts | adblock
categories = ["gambling"]
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test                  # unit + integration (network/Postgres tests are gated)
```

Gated tests:

- `BRDNS_TEST_DATABASE_URL=postgres://...` — Postgres control-plane roundtrip
- `BRDNS_TEST_NETWORK=1` — real blocklist ingestion
- `BRDNS_TEST_OTEL_ENDPOINT=http://...` — OTLP export smoke test

See [RUNBOOK.md](RUNBOOK.md) for deployment, [PRIVACY.md](PRIVACY.md) for the
privacy model, and [SECURITY.md](SECURITY.md) for the security model.
