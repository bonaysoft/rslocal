pub mod client;
pub mod server;

use std::pin::Pin;
use std::task::{Context, Poll};
use futures_core::ready;
use log::debug;
use rand::{Rng, thread_rng};
use rand::distributions::Alphanumeric;
use tokio::io;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::PollSender;
use crate::server::api::{TransferBody, TransferReply, TStatus};
use crate::server::XData;

fn random_string(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}

struct RxReader<T> {
    rx: Receiver<T>,
}

struct TxWriter<T> {
    conn_id: String,
    tx: PollSender<T>,
}

impl AsyncRead for RxReader<TransferReply> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        debug!(":poll_read");

        match ready!(self.rx.poll_recv(cx)) {
            None => {}
            Some(tr) => {
                debug!("{:?}:poll_recv", tr.conn_id);
                let data = tr.req_data;
                debug!(":poll_read_data: {:?}", String::from_utf8_lossy(&data[..data.len().min(1024)]));
                buf.put_slice(data.as_slice());
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for TxWriter<TransferBody> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        debug!("{:?}:poll_write:{}", self.conn_id, buf.len());
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        match ready!(self.tx.poll_reserve(cx)) {
            Ok(_) => {
                let conn_id = self.conn_id.clone();
                let result = self.tx.send_item(TransferBody {
                    conn_id,
                    status: TStatus::Working as i32,
                    resp_data: buf.to_vec(),
                });
                if let Err(err) = result {
                    debug!("{:?}:poll_write err: {}", self.conn_id, err.to_string());
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, err)));
                }
            }
            Err(_) => {}
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        debug!("{:?}:poll_shutdown", self.conn_id);
        match ready!(self.tx.poll_reserve(cx)) {
            Ok(_) => {
                let conn_id = self.conn_id.clone();
                let result = self.tx.send_item(TransferBody {
                    conn_id,
                    status: TStatus::Done as i32,
                    resp_data: vec![],
                });
                if let Err(err) = result {
                    debug!("{:?}:poll_shutdown err: {}", self.conn_id, err.to_string());
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, err)));
                }
            }
            Err(_) => {}
        }
        Poll::Ready(Ok(()))
    }
}

// todo 如何使用泛型进一步优化？
impl AsyncRead for RxReader<XData> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        debug!(":poll_read");

        match ready!(self.rx.poll_recv(cx)) {
            None => {}
            Some(tr) => {
                if let XData::Data(data) = tr {
                    debug!(":poll_read_data: {:?}", String::from_utf8_lossy(&data[..data.len().min(20)]));
                    buf.put_slice(data.as_slice());
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for TxWriter<Vec<u8>> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, std::io::Error>> {
        debug!("{:?}:poll_write:{}", self.conn_id, buf.len());
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        match ready!(self.tx.poll_reserve(cx)) {
            Ok(_) => {
                let result = self.tx.send_item(Vec::from(buf));
                if let Err(err) = result {
                    debug!("{:?}:poll_write err: {}", self.conn_id, err.to_string());
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, err)));
                }
            }
            Err(_) => {}
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        debug!("{:?}:poll_shutdown", self.conn_id);
        match ready!(self.tx.poll_reserve(cx)) {
            Ok(_) => {
                let result = self.tx.send_item(vec![]);
                if let Err(err) = result {
                    debug!("{:?}:poll_shutdown err: {}", self.conn_id, err.to_string());
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, err)));
                }
            }
            Err(_) => {}
        }
        Poll::Ready(Ok(()))
    }
}