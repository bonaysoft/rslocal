use std::thread;
use rslocal::server;

fn main() {
    env_logger::init();

    // webServer
    thread::spawn(|| {
        server::webserver().unwrap();
    });

    // grpcServer
    server::grpc_serve().unwrap();
}
