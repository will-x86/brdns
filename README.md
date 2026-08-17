# brdns


`brdns` is a DoT/DoH proxy that identifies each **account** by the SNI
subdomain the client dials, and applies per-account policy: ordered
allow/deny/limit rules, per-category quotas, community blocklists, and custom
upstreams.

Built on [pingora](https://github.com/cloudflare/pingora), with an axum +
PG control plane.

## Identity via sni

Both DoT and DoH read the TLS **SNI**

- DoH endpoint: `https://{account}.dns.yourdomain.com/dns-query`
- DoT endpoint: `{account}.dns.yourdomain.com:853`

## Quick start

```bash
cargo run --bin s
```


## Management

Enable control plane:

```toml
[control_plane]
enabled = true
listen_addr = "127.0.0.1:8080"
api_token = "uh openssl rand -hex 32 i guess"   # or BRDNS__CONTROL_PLANE__API_TOKEN
```

Then manage with `brdns-admin`:

```bash
export BRDNS_ADMIN_TOKEN=your-secret-token

# Create an account (prints a 16-digit number)
brdns-admin account

# Block ads by category
brdns-admin rules 1234567890123456 add --action deny --target category ads

# Limit yt to 10,000 queries/month
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

Rules are evaluated in order, first match wins

- `domain`  — exact match
- `wildcard` — `*.example.com` (matches the domain and subdomains)
- `category` — membership in a blocklist category (e.g. `ads`, `tracking`)

`limit` rules count queries per window (`hour|day|week|month`).

## Configuration

All keys optional
`brdns.toml` or `BRDNS_*` env vars. See `example.brdns.toml` for example

## Observability

- **Prometheus** `/metrics`: queries by account/outcome/protocol, latency
  histograms, blocked counters, and gauge sizes. Aggregate only.
- **OpenTelemetry** traces 

## Blocklists

Built-in, license-reviewed feeds: OISD, StevenBlack, Hagezi, AdGuard DNS 

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
cargo test
```

