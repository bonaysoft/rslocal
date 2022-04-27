use std::sync::{mpsc, Mutex};
use std::collections::HashMap;
use hyper::{Body, HeaderMap};

use lazy_static::lazy_static;
use tokio::sync::mpsc::{channel, Sender};
use crate::client::api::ProxyRequest;

#[derive(Debug)]
pub struct Site {
    // rx: Receiver<>,
}

impl Site {
    pub(crate) fn response(&self, reqHeader: &HeaderMap, reqBody: &Body) -> Result<(String), ()> {
        // 通过Channel向目标服务转发请求

        // let site = VHOST.lock().unwrap().get(host.as_str()).unwrap();
        // let tx = site.GetSender();
        //
        // // todo 将body转发给找到的目标地址
        // tx.send(); // 发送request
        //
        // rx.receive(); // 接收response

        todo!()
    }

    //
    // pub async fn waitRequest(&self, tx: Sender<Result<ProxyRequest, _>>) {
    //     let (tx2, rx2) = channel(128);
    //
    //     // 接收请求数据并转发给目标服务
    //     for rx in rx2 {
    //         tx.send(Ok(ProxyRequest { req_id: "".to_string(), data: rx })).await;
    //     }
    // }
}

// lazy_static! {
//     pub static ref VHOST: Mutex<HashMap<String, Site>> = Mutex::new(HashMap::new());
// }


// ch1: grpc-server -> grpc-client
// ch2: webserver -> grpc-server -> grpc-client -> webserver