use std::collections::HashMap;
use std::sync::Mutex;
use lazy_static::lazy_static;
use tokio::{sync};
use tokio::sync::{mpsc};
use tokio::net::{TcpStream};
use tokio::sync::mpsc::Sender;
use tonic::Status;
use crate::server::{Connection, grpc};

// #[derive(Debug, Clone)]
// pub struct R {
//     pub header_tx: mpsc::Sender<Vec<u8>>,
//     pub body_tx: mpsc::Sender<Result<Vec<u8>, Status>>,
// }

lazy_static! {
    // pub static ref VHOST: Mutex<HashMap<String, mpsc::Sender<Connection>>> = Mutex::new(HashMap::new());
    // pub static ref RR: Mutex<HashMap<String, R>> = Mutex::new(HashMap::new());
    pub static ref CONNS: Mutex<HashMap<String, Connection>> = Mutex::new(HashMap::new());
}

pub fn conns_get(id: String) -> Option<Connection> {
    if let Some(r) = CONNS.lock().unwrap().get(id.as_str()) {
        Some(r.clone())
    } else {
        None
    }
}

// pub fn rr_get(rid: String) -> Option<R> {
//     if let Some(r) = RR.lock().unwrap().get(rid.as_str()) {
//         Some(r.clone())
//     } else {
//         None
//     }
// }
//
// pub fn rr_set(rid: String, r: R) -> bool {
//     if get_site_host(rid.clone()).is_some() {
//         return false;
//     }
//
//     RR.lock().unwrap().insert(rid, r);
//     return true;
// }

// pub fn get_site_host(host: String) -> Option<mpsc::Sender<Connection>> {
//     if let Some(site) = VHOST.lock().unwrap().get(host.as_str()) {
//         Some(site.clone())
//     } else {
//         None
//     }
// }

// pub fn setup_site_host(host: String, otx: Sender<Connection>) -> bool {
//     if get_site_host(host.clone()).is_some() {
//         return false;
//     }
//
//     VHOST.lock().unwrap().insert(host, otx);
//     return true;
// }
//
// pub fn remove_site_host(host: String) {
//     VHOST.lock().unwrap().remove(host.as_str());
// }
//

