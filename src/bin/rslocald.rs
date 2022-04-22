use rslocal::server;

fn main() {
    server::run();
}

// client <= config at HOME
// server <= flag and env

// client连接server，输出服务端域名地址
// server接收外部请求，将请求转发给client