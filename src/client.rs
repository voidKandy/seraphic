use crossbeam_channel::TryRecvError;
use tracing::{debug, Level};

use crate::{
    connection::{
        endpoint::TcpEndpoint, io::packet::MessagePacket, Connection, InitializeConnectionMessage,
    },
    error::{ErrorCode, ErrorKind},
    MainResult, Message, Response, RpcRequest, RpcResponse,
};
use std::{
    io::BufReader,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct ClientConnection<I> {
    pub conn: Connection<I>,
    pub local_addr: SocketAddr,
}

impl<I> TcpEndpoint for ClientConnection<I>
where
    I: InitializeConnectionMessage,
{
    type InitializeReq = I;
    const CHANNEL_BUFFER_SIZE: usize = 5;

    fn reader_thread_logic(
        stream: TcpStream,
        reader_sender: crossbeam_channel::Sender<crate::Message>,
        shutdown_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> std::io::Result<()> {
        let reader_span = tracing::span!(Level::INFO, "client_reader");
        let _enter = reader_span.enter();

        let mut buf_read = BufReader::new(stream);
        while !shutdown_signal.load(Ordering::Relaxed) {
            if let Some(msg) = MessagePacket::read(&mut buf_read).unwrap() {
                tracing::debug!("got message: {msg:#?}");
                reader_sender.send(msg).unwrap();
            }
        }
        tracing::debug!("leaving reader");
        Ok(())
    }

    fn writer_thread_logic(
        mut stream: TcpStream,
        writer_receiver: crossbeam_channel::Receiver<Message>,
        shutdown_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> std::io::Result<()> {
        let writer_span = tracing::span!(Level::INFO, "client_writer");
        let _enter = writer_span.enter();
        let mut should_shutdown = false;
        while !shutdown_signal.load(Ordering::Relaxed) {
            match writer_receiver.try_recv() {
                Ok(msg) => {
                    should_shutdown = matches!(msg, Message::Exit);
                    MessagePacket::write(&mut stream, &msg)?;
                }
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(TryRecvError::Disconnected) => {
                    should_shutdown = true;
                }
            }

            if should_shutdown {
                tracing::debug!("writer should shutdown");
                shutdown_signal.store(true, Ordering::Relaxed);
            }
        }
        tracing::debug!("leaving writer");
        Ok(())
    }
}

impl<I> ClientConnection<I>
where
    I: InitializeConnectionMessage,
{
    pub fn connect(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        tracing::debug!("client connected!");
        let local_addr = stream.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let conn = Self::socket_transport_connection(stream, shutdown);
        Ok(Self { conn, local_addr })
    }

    // pub fn shutdown(&self) {
    //     self.conn
    //         .io_threads
    //         .shutdown_signal
    //         .store(true, Ordering::Relaxed);
    // }

    /// Initializes the connection with a server by sending an initialization request
    /// Hangs until an inialization response is returned
    /// Outer Errors if serialization fails
    /// Inner Errors if an unexpected response is returned
    pub fn initialize(&self, req: I) -> MainResult<Result<Response, crate::error::Error>> {
        tracing::debug!("client initializing");
        self.conn
            .sender
            .send(req.init_request().into())
            .expect("failed to send");

        match self.conn.receiver.recv() {
            Ok(msg) => {
                tracing::debug!("client received: {msg:#?} in initialization");

                match msg {
                    crate::Message::Res(r) if I::matches(&r) => {
                        match <I as RpcRequest>::Response::try_from_response(&r)? {
                            Ok(res) => Ok(Ok(I::init_response(res))),
                            Err(e) => Ok(Err(e)),
                        }
                    }
                    _ => {
                        return Ok(Err(ErrorKind::other(
                            &format!(r#"expected Initialize response, got: {msg:?}"#),
                            ErrorCode::ServerErrorStart,
                        )
                        .into()));
                    }
                }
            }
            Err(err) => {
                return Err(std::io::Error::other(format!("receive error: {err:#?}")).into())
            }
        }
    }
}
