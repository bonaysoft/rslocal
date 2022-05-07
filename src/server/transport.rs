use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct Payload {
    pub tx: Sender<Connection>,
    pub entrypoint: String,
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub(crate) id: String,
    pub(crate) tx: Sender<XData>,
}

#[derive(Debug, Clone)]
pub enum XData {
    TX(Sender<Vec<u8>>),
    Data(Vec<u8>),
}