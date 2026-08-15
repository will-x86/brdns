# brdns

Encrypted DNS proxy and query tool in Rust.

## What it does

- **`s` (Server) ** — DNS-over-TLS (DoT) and DNS-over-HTTPS (DoH) forwarding proxy. Listens for encrypted DNS queries and relays them to an upstream resolver. Built on [pingora](https://github.com/cloudflare/pingora).
- **`g` (Client for req) ** — DNS query client supporting UDP, DoT, and DoH transports.

## Quick start

```bash
# Run the DoT proxy (default port 8853 -> upstream 1.1.1.1:853)
cargo run --bin s dot

# Run the DoH proxy (default port 6188 -> upstream cloudflare-dns.com)
cargo run --bin s doh

# Query google.com via DoH
cargo run --bin g doh -d google.com
```

## Configuration

Optional `brdns.toml` in the working directory, or `BRDNS_*` env vars. All keys are optional; defaults use Cloudflare/Google upstreams.

```toml
[dot]
listen_port = 8853
upstream_host = "1.1.1.1"
upstream_port = 853

[doh]
listen_port = 6188
upstream_host = "cloudflare-dns.com"
upstream_addr = "1.1.1.1:443"

[udp]
server = "8.8.8.8"
port = 53

[certs]
in_mem = true      # keep auto-generated certs in memory
```

Self-signed certs are auto-generated on startup (RSA 2048-bit, SAN: localhost/127.0.0.1). Or supply your own via `cert_path`/`key_path`.

