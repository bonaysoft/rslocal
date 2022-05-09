# rslocal

[English](README.md) | [中文](README_zh.md)

[![](https://github.com/saltbo/rslocal/workflows/build/badge.svg)](https://github.com/saltbo/rslocal/actions?query=workflow%3Abuild)
[![](https://img.shields.io/crates/v/rslocal.svg?color=orange)](https://crates.io/crates/rslocal)
[![](https://img.shields.io/github/v/release/saltbo/rslocal.svg?color=brightgreen)](https://github.com/saltbo/rslocal/releases)
[![](https://img.shields.io/github/license/saltbo/rslocal?color=blue)](https://github.com/saltbo/rslocal/blob/master/LICENSE)

## rslocal是什么?

Rslocal是一个类似ngrok的Rust实现，使用它可以很方便的构建一条内网穿透隧道。

## 项目状态

- [x] 支持HTTP协议
- [x] 支持TCP协议
- [ ] 支持UDP协议
- [x] 支持Token登录
- [ ] 支持OIDC登录
- [ ] 支持连接断开重连
- [ ] 客户端支持输出访问日志

## Rslocal

运行在本地的客户端程序，用于接收服务器请求并转发给本地的服务

### 安装

Mac用户

```shell
brew install saltbo/bin/rslocal
```

其他用户（该脚本暂不支持Windows，需要[手动下载](https://github.com/saltbo/rslocal/releases)）

```shell
curl -sSf https://raw.githubusercontent.com/saltbo/rslocal/master/install.sh | sh
```

### 使用

```shell
rslocal config
rslocal http 8000
rslocal tcp 18000
```

## Rslocald

服务端程序，用于接收外部请求并转发给rslocal

### 云服务

访问 [localtest.rs](https://localtest.rs) 了解详情

### 自建服务

```shell
mkdir /etc/rslocal
touch /etc/rslocal/rslocald.toml
#edit your config like example configfile
docker run -it -p 8422:8422 -p 8423:8423 -v /etc/rslocal:/etc/rslocal saltbo/rslocald
```

### 服务端配置文件样例

```toml
[core]
[core]
debug = false
bind_addr = "0.0.0.0:8422"
auth_method = "token"  # token, oidc
allow_ports = "18000-19000"

[http]
bind_addr = "0.0.0.0:8423"
default_domain = "localtest.me:8423"
#default_static = "/etc/rslocal/webroot"

[tokens]
bob = "rslocald_abc11"
alice = "rslocald_abc32"

#[oidc]
#issuer = ""
#audience = ""
```

## 参与贡献

1. 搜索代码中的todo和fixme标记，解决它
2. 实现项目状态中没有打钩的选项

## 特别感谢

- [Rust众](https://t.me/rust_zh)
- [布丁 包](https://github.com/bdbai)
- [Pop](https://github.com/George-Miao)
- [Space](https://github.com/spacemeowx2)

## 开源协议

rslocal is under the Apache-2.0 license. See the [LICENSE](/LICENSE) file for details.

[![Stargazers over time](https://starchart.cc/saltbo/rslocal.svg)](https://starchart.cc/saltbo/rslocal)
