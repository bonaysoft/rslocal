# localtest.rs

## Server
- endpoint: `http://rs.localtest.rs:8422`
- token: `rslocal666`

## Usage
```shell
➜  ~ rslocal config
? server endpoint? http://rs.localtest.rs:8422
? authorization token? rslocal666
config saved at "/Users/saltbo/.config/rslocal/config.ini"
```

## Self-hosted

### Deploy

```shell
mkdir /etc/rslocal
touch /etc/rslocal/rslocald.toml
#edit your config like example configfile

docker run -it -p 8422:8422 -p 8423:8423 -v /etc/rslocal:/etc/rslocal saltbo/rslocald
```

### Configfile

The `rslocald.toml` file is required for `rslocald`.

```toml
[core]
debug = false
bind_addr = "0.0.0.0:8422"
auth_method = "token"  # token, oidc
allow_ports = "18000-19000"

[http]
bind_addr = "0.0.0.0:8423"
default_domain = "example.com"
# default_static = "/etc/rslocal/webroot" # support later

[tokens]
bob = "rslocald_abc11"
alice = "rslocald_abc32"

#[oidc]
#issuer = ""
#audience = ""
```