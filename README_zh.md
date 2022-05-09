# rslocal

[English](README.md) | [中文](README_zh.md)

[![](https://github.com/saltbo/rslocal/workflows/build/badge.svg)](https://github.com/saltbo/rslocal/actions?query=workflow%3Abuild)
[![](https://img.shields.io/github/v/release/saltbo/rslocal.svg)](https://github.com/saltbo/rslocal/releases)
[![](https://img.shields.io/github/license/saltbo/rslocal.svg)](https://github.com/saltbo/rslocal/blob/master/LICENSE)

## What is rslocal?

Rslocal是一个类似ngrok的Rust实现，使用它可以很方便的构建一条内网穿透隧道。

## Project status

- [x] 支持HTTP协议
- [x] 支持TCP协议
- [ ] 支持UTP协议
- [x] 支持Token登录
- [ ] 支持OIDC登录
- [ ] 支持连接断开重连
- [ ] 客户端支持输出访问日志

## Rslocal

运行在本地的客户端程序，用于接收服务器请求并转发给本地的服务

### Setup

```shell
curl -sSf https://raw.githubusercontent.com/saltbo/rslocal/master/install.sh | sh
```

### Configfile

```toml
endpoint = "localtest.rs:8422"
token = "rslocald_abc32"
```

## Rslocald

服务端程序，用于接收外部请求并转发给rslocal

### Deploy

[![Deploy](https://www.herokucdn.com/deploy/button.svg)](https://heroku.com/deploy?template=https://github.com/saltbo/rslocal)

### Configfile

```toml
[core]
debug = false
bind_addr = "[::1]:8422"
auth_method = "token"  # token, oidc
allow_ports = "18000-19000"

[http]
bind_addr = "[::1]:8423"
default_domain = "example.com"
default_static = "/opt/rslocald/webroot" #未实现

[tokens]
bob = "rslocald_abc11"
alice = "rslocald_abc32"
```

## Special thanks

- [Rust众](https://t.me/rust_zh)
- [布丁 包](https://github.com/bdbai)
- [Pop](https://github.com/George-Miao)
- [Space](https://github.com/spacemeowx2)

## Contributing

1. write code for the todo and fixme tag
2. implement the unchecked item of the Project status

## License

rslocal is under the Apache-2.0 license. See the [LICENSE](/LICENSE) file for details.

[![Stargazers over time](https://starchart.cc/saltbo/rslocal.svg)](https://starchart.cc/saltbo/rslocal)
