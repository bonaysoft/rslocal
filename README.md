# rslocal

[English](README.md) | [中文](README_zh.md)

[![](https://github.com/saltbo/rslocal/workflows/build/badge.svg)](https://github.com/saltbo/rslocal/actions?query=workflow%3Abuild)
[![](https://img.shields.io/github/v/release/saltbo/rslocal.svg)](https://github.com/saltbo/rslocal/releases)
[![](https://img.shields.io/github/license/saltbo/rslocal.svg)](https://github.com/saltbo/rslocal/blob/master/LICENSE)

## What is rslocal?

Rslocal is like ngrok built in Rust, it builds a tunnel to localhost.

## Project status

- [x] support http
- [x] support tcp
- [ ] support udp
- [x] support token login
- [ ] support oidc login
- [ ] disconnection reconnect
- [ ] access log for client

## Rslocal

A client program that runs locally to receive server requests and forward them to local services

### Installation

MacOS

```shell
brew install saltbo/bin/rslocal
```

OtherOS

```shell
curl -sSf https://raw.githubusercontent.com/saltbo/rslocal/master/install.sh | sh
```

### Usage

```shell
rslocal config
rslocal http 8000
rslocal tcp 18000
```

## Rslocald

Server program that receives external requests and forwards them to `rslocal`

### Cloud-service

Visit [localtest.rs](https://localtest.rs)

### Self-hosted

```shell
mkdir /etc/rslocal
touch /etc/rslocal/rslocald.toml
#edit your config like example configfile

docker run -it -p 8422:8422 -p 8423:8423 -v /opt/rslocal:/etc/rslocal saltbo/rslocald
docker run -it -p 8422:8422 -p 8423:8423 -v /etc/rslocal:/etc/rslocal saltbo/rslocald
```

### Configfile

This root `rslocald.toml` file is required for `rslocald`.

```toml
[core]
debug = false
bind_addr = "0.0.0.0:8422"
auth_method = "token"  # token, oidc
allow_ports = "18000-19000"

[http]
bind_addr = "0.0.0.0:8423"
default_domain = "example.com"
# default_static = "/opt/rslocald/webroot" # support later

[tokens]
bob = "rslocald_abc11"
alice = "rslocald_abc32"
```

## Contributing

1. write code for the todo and fixme tag
2. implement the unchecked item of the Project status

## Special thanks

- [rust_zh](https://t.me/rust_zh)
- [bdbai](https://github.com/bdbai)
- [spacemeowx2](https://github.com/spacemeowx2)
- [Pop](https://t.me/Pop_gg)

## License

rslocal is under the Apache-2.0 license. See the [LICENSE](/LICENSE) file for details.

[![Stargazers over time](https://starchart.cc/saltbo/rslocal.svg)](https://starchart.cc/saltbo/rslocal)
