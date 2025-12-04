use std::{
    future::Future, io, net::SocketAddr, pin::Pin, sync::Arc, task::{Context, Poll, ready}, thread::{self, sleep}, time::{Duration, Instant}
};

use tokio::{
    io::Interest, time::{Sleep, sleep_until}
};

use crate::Runtime;

use super::{AsyncTimer, AsyncUdpSocket, UdpSenderHelper, UdpSenderHelperSocket};

/// A Quinn runtime for Tokio
#[derive(Debug)]
pub struct TokioRuntime;

impl Runtime for TokioRuntime {
    fn new_timer(&self, t: Instant) -> Pin<Box<dyn AsyncTimer>> {
        Box::pin(sleep_until(t.into()))
    }

    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        tokio::spawn(future);
    }

    fn wrap_udp_socket(&self, sock: std::net::UdpSocket) -> io::Result<Box<dyn AsyncUdpSocket>> {
        Ok(Box::new(UdpSocket {
            inner: Arc::new(udp::UdpSocketState::new((&sock).into())?),
            io: Arc::new(tokio::net::UdpSocket::from_std(sock)?),
        }))
    }

    fn now(&self) -> Instant {
        tokio::time::Instant::now().into_std()
    }
}

impl AsyncTimer for Sleep {
    fn reset(self: Pin<&mut Self>, t: Instant) {
        Self::reset(self, t.into())
    }
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        Future::poll(self, cx)
    }
}

#[derive(Debug, Clone)]
struct UdpSocket {
    io: Arc<tokio::net::UdpSocket>,
    inner: Arc<udp::UdpSocketState>,
}

impl UdpSenderHelperSocket for UdpSocket {
    fn max_transmit_segments(&self) -> usize {
        self.inner.max_gso_segments()
    }

    fn try_send(&self, transmit: &udp::Transmit) -> io::Result<()> {
        self.io.try_io(Interest::WRITABLE, || {
            self.inner.send((&self.io).into(), transmit)
        })
    }
}

impl AsyncUdpSocket for UdpSocket {
    fn create_sender(&self) -> Pin<Box<dyn super::UdpSender>> {
        Box::pin(UdpSenderHelper::new(self.clone(), |socket: &Self| {
            let socket = socket.clone();
            async move { socket.io.writable().await }
        }))
    }

    fn poll_recv(
        &mut self,
        cx: &mut Context,
        bufs: &mut [std::io::IoSliceMut<'_>],
        meta: &mut [udp::RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            ready!(self.io.poll_recv_ready(cx))?;
            if let Ok(res) = self.io.try_io(Interest::READABLE, || {
                self.inner.recv((&self.io).into(), bufs, meta)
            }) {
                return Poll::Ready(Ok(res));
            }
        }
    }

    fn local_addr(&self) -> io::Result<std::net::SocketAddr> {
        self.io.local_addr()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.gro_segments()
    }

    fn get_local_port(&self) -> u16 {
        self.io.local_addr().unwrap().port()
    }

    fn send_to(&self, buf: Vec<u8>, addr: SocketAddr) {
        let io_socket = self.io.clone();
        thread::spawn(move || { 
            let tokio_runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let _runtime_guard = tokio_runtime.enter();
            
            println!("actually sending holepunch packets...");
            let spam_interval_millis = 50;
            for _milliseconds in (0..=20_000).step_by(spam_interval_millis as usize) {
                let result = tokio_runtime.block_on(async { 
                    io_socket.send_to(&buf, addr).await
                }).unwrap();
                assert_eq!(result, buf.len());
                sleep(Duration::from_millis(spam_interval_millis));
            }
            println!("completed holepunch packets burst");
        });
    }
}
