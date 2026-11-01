//! Connection lifecycle hooks for the tonic accept loop.
//!
//! A supervisor (e.g. ctl-masterd) needs to know when a child is serving a
//! connection so it can scale the process pool. tonic's `TcpIncoming` accepts
//! internally, so the only way to observe connections without changing the
//! transport is to wrap the incoming stream and the accepted IO.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_core::Stream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tonic::transport::server::Connected;

/// Connection lifecycle event delivered to a [`ConnectionHook`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    /// A TCP connection was accepted and handed to the server.
    Opened,
    /// The connection's IO was dropped (connection finished or failed).
    Closed,
}

/// Called once per accepted connection (`Opened`) and once when that
/// connection's IO is dropped (`Closed`).
///
/// Hooks must be cheap: they run inline in the accept loop / on drop and must
/// not block.
pub type ConnectionHook = Arc<dyn Fn(ConnectionEvent) + Send + Sync>;

/// Adapts a stream of accepted sockets into a stream of hooked sockets.
pub struct HookedIncoming<S> {
    inner: S,
    hook: Option<ConnectionHook>,
}

/// Wrap any stream of accepted TCP connections so that `Opened`/`Closed`
/// events reach `hook`. Useful for servers that drive tonic directly instead of
/// using [`crate::Server::serve_with_listener`].
pub fn tracked_incoming<S, E>(incoming: S, hook: Option<ConnectionHook>) -> HookedIncoming<S>
where
    S: Stream<Item = Result<tokio::net::TcpStream, E>> + Unpin,
{
    HookedIncoming::new(incoming, hook)
}

impl<S> HookedIncoming<S> {
    pub fn new(inner: S, hook: Option<ConnectionHook>) -> Self {
        Self { inner, hook }
    }
}

impl<S, E> Stream for HookedIncoming<S>
where
    S: Stream<Item = Result<tokio::net::TcpStream, E>> + Unpin,
{
    type Item = Result<HookedStream, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(stream))) => {
                if let Some(hook) = &this.hook {
                    hook(ConnectionEvent::Opened);
                }
                Poll::Ready(Some(Ok(HookedStream {
                    inner: stream,
                    hook: this.hook.clone(),
                })))
            }
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// A `TcpStream` that reports `Closed` when it is dropped.
pub struct HookedStream {
    inner: tokio::net::TcpStream,
    hook: Option<ConnectionHook>,
}

impl AsyncRead for HookedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for HookedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

impl Connected for HookedStream {
    type ConnectInfo = <tokio::net::TcpStream as Connected>::ConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        self.inner.connect_info()
    }
}

impl Drop for HookedStream {
    fn drop(&mut self) {
        if let Some(hook) = &self.hook {
            hook(ConnectionEvent::Closed);
        }
    }
}
