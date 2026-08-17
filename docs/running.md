

## 1 Wildcard cert

You need a wildcard for your domain. 
E.g.

```
certbot certonly --dns-cloudflare \
  --dns-cloudflare-credentials cloudflare.ini \
  -d 'dns.yourdomain.com' -d '*.dns.yourdomain.com'
```


Add to config:
```
[certs]
cert_path = "/etc/letsencrypt/live/dns.yourdomain.com/fullchain.pem"
key_path  = "/etc/letsencrypt/live/dns.yourdomain.com/privkey.pem"
in_mem = false
```


## 2 

Point `dns.domain.com` and `*.dns.domain.com` on A rec to IP

## 3 Postgres

Generally `podman compose` with `podman compose up -d` to bring it up

## 4 configuration

Customize brdns.toml and set secrets via env vars if you want e.g.:
```
export BRDNS__CONTROL_PLANE__DATABASE_URL="postgres://brdns:<password>@127.0.0.1:5432/brdns"
export BRDNS__CONTROL_PLANE__API_TOKEN="<long-random-token>"
```

(Database URL and all stuff *can* go in the config)

You can generate a token with `openssl rand -hex 32`

## 5 Run 

For dev do `make wdot`


## 6 Add someone 

```
export BRDNS_ADMIN_TOKEN=...
cargo run --bin brdns-admin account
```


## 7 Monitoring

- Prometheus scrape `127.0.0.1:9090/metrics`.
- Optional OTel collector in config
- `RUST_LOG=info` for structured JSON logs.



