use anyhow::{Context, Result};
use dubbo_rs_common::url::URL;
use dubbo_rs_protocol::Invoker;
use dubbo_rs_remoting::{Codec, ExchangeClient, ExchangeServer, Request, Response};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use std::sync::Arc;

use crate::codec::{DubboCodec, SerializationId, HEADER_LENGTH};

pub struct DubboServer {
    invoker: Option<Arc<dyn Invoker>>,
    serialization_id: u8,
    shutdown: Arc<tokio::sync::Notify>,
}

impl DubboServer {
    #[must_use]
    pub fn new(serialization_id: SerializationId) -> Self {
        Self {
            invoker: None,
            serialization_id: serialization_id.to_u8(),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        }
    }

    #[must_use]
    pub fn with_invoker(mut self, invoker: Arc<dyn Invoker>) -> Self {
        self.invoker = Some(invoker);
        self
    }
}

#[async_trait::async_trait]
impl ExchangeServer for DubboServer {
    async fn bind(&self, url: &URL) -> Result<()> {
        let addr = url.get_address();
        let socket = tokio::net::TcpSocket::new_v4()?;
        socket.set_reuseaddr(true)?;
        socket.bind(addr.parse().with_context(|| format!("invalid address: {addr}"))?)?;
        let listener = socket.listen(128)?;

        let invoker = self.invoker.clone();
        let serial_id =
            SerializationId::from_u8(self.serialization_id).unwrap_or(SerializationId::Hessian2);
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            let shutdown = shutdown.notified();
            tokio::pin!(shutdown);
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let (mut stream, _peer_addr) = match result {
                            Ok(conn) => conn,
                            Err(_) => continue,
                        };
                        let codec = DubboCodec::new(serial_id);
                        let inv = invoker.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(&mut stream, &codec, inv).await {
                                eprintln!("connection handler error: {e}");
                            }
                        });
                    }
                    _ = &mut shutdown => {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn close(&self) {
        self.shutdown.notify_one();
    }
}

use crate::body;

async fn handle_connection(
    stream: &mut TcpStream,
    codec: &DubboCodec,
    invoker: Option<Arc<dyn Invoker>>,
) -> Result<()> {
    use anyhow::bail;
    use dubbo_rs_common::constants;

    loop {
        let mut header = [0u8; HEADER_LENGTH];
        if let Err(e) = stream.read_exact(&mut header).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
            bail!("read header error: {e}");
        }

        let body_len =
            u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize;

        let mut frame = header.to_vec();
        if body_len > 0 {
            let mut body = vec![0u8; body_len];
            stream
                .read_exact(&mut body)
                .await
                .context("failed to read body")?;
            frame.extend_from_slice(&body);
        }

        let req = codec.decode_request(&frame)?;

        let resp = if let Some(ref inv) = invoker {
            match body::decode_request_body(&req.data, inv.get_url()) {
                Ok(mut ctx) => match inv.invoke(&mut ctx).await {
                    Ok(rpc_result) => match body::encode_response_body(&rpc_result) {
                        Ok(resp_data) => Response::success(req.id, resp_data),
                        Err(e) => Response::error(
                            req.id,
                            constants::SERVER_ERROR_STATUS,
                            format!("encode error: {e}").into_bytes(),
                        ),
                    },
                    Err(e) => Response::error(
                        req.id,
                        constants::SERVER_ERROR_STATUS,
                        format!("invoke error: {e}").into_bytes(),
                    ),
                },
                Err(e) => Response::error(
                    req.id,
                    constants::BAD_REQUEST_STATUS,
                    format!("decode error: {e}").into_bytes(),
                ),
            }
        } else {
            Response::success(req.id, vec![])
        };

        let resp_frame = codec.encode_response(&resp)?;
        stream
            .write_all(&resp_frame)
            .await
            .context("failed to write response")?;
    }

    Ok(())
}

async fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>> {
    let mut header = [0u8; HEADER_LENGTH];
    stream
        .read_exact(&mut header)
        .await
        .context("failed to read Dubbo frame header")?;

    let body_len = u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize;

    let mut frame = header.to_vec();
    if body_len > 0 {
        let mut body = vec![0u8; body_len];
        stream
            .read_exact(&mut body)
            .await
            .context("failed to read Dubbo frame body")?;
        frame.extend_from_slice(&body);
    }

    Ok(frame)
}

pub struct DubboClient {
    stream: Mutex<Option<TcpStream>>,
    codec: DubboCodec,
}

impl DubboClient {
    #[must_use]
    pub fn new(serialization_id: SerializationId) -> Self {
        Self {
            stream: Mutex::new(None),
            codec: DubboCodec::new(serialization_id),
        }
    }
}

#[async_trait::async_trait]
impl ExchangeClient for DubboClient {
    async fn connect(&mut self, url: &URL) -> Result<()> {
        let addr = url.get_address();
        let stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("failed to connect to {addr}"))?;
        stream.set_nodelay(true)?;
        *self.stream.get_mut() = Some(stream);
        Ok(())
    }

    async fn request(&self, req: Request) -> Result<Response> {
        let mut guard = self.stream.lock().await;
        let stream = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("not connected — call connect() first"))?;

        let frame = self.codec.encode_request(&req)?;
        stream
            .write_all(&frame)
            .await
            .context("failed to write request frame")?;

        let response_frame = read_frame(stream).await?;
        self.codec.decode_response(&response_frame)
    }

    fn close(&self) {
        if let Ok(mut guard) = self.stream.try_lock() {
            *guard = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener as TokioTcpListener;

    fn get_available_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind to auto port");
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn test_client_connect_to_server() {
        let port = get_available_port();
        let listener = TokioTcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("bind");

        let mut url = URL::new("dubbo", "/test");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let handle = tokio::spawn(async move {
            let _ = listener.accept().await.expect("accept");
        });

        let mut client = DubboClient::new(SerializationId::Hessian2);
        client.connect(&url).await.expect("connect should succeed");

        handle.await.expect("listener task");
    }

    #[tokio::test]
    async fn test_client_request_response_roundtrip() {
        let port = get_available_port();
        let listener = TokioTcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("bind");

        let mut url = URL::new("dubbo", "/test");
        url.ip = "127.0.0.1".into();
        url.port = port.to_string();

        let codec = DubboCodec::new(SerializationId::Hessian2);

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");

            let mut header = [0u8; HEADER_LENGTH];
            stream.read_exact(&mut header).await.expect("read header");

            let body_len =
                u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize;

            let mut frame = header.to_vec();
            if body_len > 0 {
                let mut body = vec![0u8; body_len];
                stream.read_exact(&mut body).await.expect("read body");
                frame.extend_from_slice(&body);
            }

            let req = codec.decode_request(&frame).expect("decode request");

            let resp = Response::success(req.id, b"echo:hello".to_vec());
            let resp_frame = codec.encode_response(&resp).expect("encode response");
            stream.write_all(&resp_frame).await.expect("write response");
        });

        let mut client = DubboClient::new(SerializationId::Hessian2);
        client.connect(&url).await.expect("connect");

        let req = Request {
            id: 42,
            is_twoway: true,
            is_event: false,
            data: b"hello".to_vec(),
        };

        let resp = client.request(req).await.expect("request should succeed");
        assert_eq!(resp.id, 42);
        assert_eq!(resp.status, 20);
        assert_eq!(resp.data, b"echo:hello");

        handle.await.expect("listener task");
    }

    #[tokio::test]
    async fn test_client_request_before_connect_error() {
        let client = DubboClient::new(SerializationId::Hessian2);
        let req = Request {
            id: 1,
            is_twoway: true,
            is_event: false,
            data: vec![],
        };

        let result = client.request(req).await;
        assert!(result.is_err());
    }
}
