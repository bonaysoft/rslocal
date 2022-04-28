use std::thread;
use rslocal::server;

fn main() {
    thread::spawn(|| {
        // tx交给webserver，用来发送http请求
        server::webserver();
    });

    server::run().unwrap();
}

// client <= config at HOME
// server <= flag and env

// client连接server，输出服务端域名地址
// server接收外部请求，将请求转发给client