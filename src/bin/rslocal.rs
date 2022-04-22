use rslocal::client;

fn main() {
    let source = config::File::with_name("config");
    let cfg = config::Config::builder().add_source(source).build();
    let ep = cfg.as_ref().unwrap().get_string("endpoint").unwrap();
    let token = cfg.as_ref().unwrap().get_string("token").unwrap();

    client::run(ep, token);
}

// client <= config at HOME
// server <= flag and env

// client连接server，输出服务端域名地址
// server接收外部请求，将请求转发给client