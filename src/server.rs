use crate::{
    connection::{
        endpoint::TcpEndpoint, io::packet::MessagePacket, Connection, InitializeConnectionMessage,
    },
    error::{Error, ErrorCode, ErrorKind},
    Message, Request, Response, RpcResponse,
};
use crossbeam_channel::{RecvTimeoutError, TryRecvError};
use std::{
    collections::HashMap,
    io::BufReader,
    marker::PhantomData,
    net::{SocketAddr, TcpListener, ToSocketAddrs},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};
use tracing::Level;

pub type ServerHandlerResult =
    std::result::Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
pub struct Server<I, H> {
    listener: TcpListener,
    connections: HashMap<SocketAddr, (JoinHandle<ServerHandlerResult>, Arc<AtomicBool>)>,
    shutdown_signal: Arc<AtomicBool>,
    _marker: PhantomData<(I, H)>,
}

pub trait ServerConnectionHandler<I>
where
    I: InitializeConnectionMessage,
    Self: 'static,
{
    fn handler(conn: &mut ServerConnection<I>) -> ServerHandlerResult;
}

pub struct ServerConnection<I> {
    pub conn: Connection<I>,
    pub client_addr: SocketAddr,
}

impl<I, H> Iterator for Server<I, H>
where
    I: InitializeConnectionMessage,
{
    type Item = std::io::Result<(ServerConnection<I>, Arc<AtomicBool>)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.shutdown_signal.load(Ordering::Relaxed) {
            tracing::warn!("server shutdown, no connection to return");
            return None;
        }
        let conn_shutdown = Arc::new(AtomicBool::new(false));
        match self.listener.accept() {
            Ok((stream, addr)) => {
                let conn = ServerConnection::socket_transport_connection(
                    stream,
                    Arc::clone(&conn_shutdown),
                );
                Some(Ok((
                    ServerConnection {
                        conn,
                        client_addr: addr,
                    },
                    conn_shutdown,
                )))
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
            Err(e) => {
                tracing::error!("Failed to accept connection: {}", e);
                Some(Err(e))
            }
        }
    }
}

impl<I> TcpEndpoint for ServerConnection<I>
where
    I: InitializeConnectionMessage,
{
    type InitializeReq = I;
    const CHANNEL_BUFFER_SIZE: usize = 5;

    fn writer_thread_logic(
        mut stream: std::net::TcpStream,
        writer_receiver: crossbeam_channel::Receiver<Message>,
        shutdown_signal: Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        let writer_span = tracing::span!(Level::INFO, "server_writer");
        let _enter = writer_span.enter();
        while !shutdown_signal.load(Ordering::Relaxed) {
            match writer_receiver.try_recv() {
                Ok(msg) => {
                    // tracing::debug!("going to send msg: {msg:#?}");
                    // if matches!(msg, Message::Exit) {
                    //     shutdown_signal.store(true, Ordering::Relaxed);
                    // }
                    MessagePacket::write(&mut stream, &msg)?;
                }
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(TryRecvError::Disconnected) => {
                    // shutdown_signal.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
        tracing::debug!("leaving writer");
        Ok(())
    }

    fn reader_thread_logic(
        stream: std::net::TcpStream,
        reader_sender: crossbeam_channel::Sender<Message>,
        shutdown_signal: Arc<AtomicBool>,
    ) -> std::io::Result<()> {
        let reader_span = tracing::span!(Level::INFO, "server_reader");
        let _enter = reader_span.enter();
        let mut buf_read = BufReader::new(stream);
        while !shutdown_signal.load(Ordering::Relaxed) {
            if let Some(msg) = MessagePacket::read(&mut buf_read).unwrap() {
                tracing::debug!("got message: {msg:#?}");
                // if matches!(msg, Message::Exit) {
                //     shutdown_signal.store(true, Ordering::Relaxed);
                // }
                reader_sender.send(msg).unwrap();
            }
        }

        tracing::debug!("leaving reader");
        Ok(())
    }
}

impl<I, H> Server<I, H>
where
    I: InitializeConnectionMessage,
    H: ServerConnectionHandler<I>,
{
    pub fn shutdown(&mut self) {
        self.shutdown_signal.store(true, Ordering::Relaxed);
    }

    pub fn connected_clients(&self) -> Vec<SocketAddr> {
        self.connections.keys().cloned().collect()
    }

    pub fn listen(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            connections: HashMap::new(),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            _marker: PhantomData,
        })
    }

    pub fn shutdown_and_join_all_connections(&mut self) -> ServerHandlerResult {
        let all_keys: Vec<SocketAddr> = self.connections.keys().cloned().collect();
        for k in all_keys {
            let (thread, shutdown) = self.connections.remove(&k).unwrap();
            shutdown.store(true, Ordering::Relaxed);
            tracing::debug!("shutting down thread for {k:#?}");
            let _ = thread.join().unwrap();
        }
        Ok(())
    }

    pub fn spawn_connection_thread(
        &mut self,
        mut conn: ServerConnection<I>,
        conn_shutdown: Arc<AtomicBool>,
    ) -> SocketAddr {
        let key = conn.client_addr.clone();
        tracing::debug!("spawning server connection handler");
        let thread = std::thread::spawn(move || {
            let span = tracing::span!(
                Level::INFO,
                "server connection span",
                client_address = conn.client_addr.to_string()
            );
            let _enter = span.enter();
            H::handler(&mut conn)?;

            tracing::debug!("exited handler, shutting down connection");
            conn.conn
                .io_threads
                .shutdown_signal
                .store(true, Ordering::Relaxed);
            conn.conn.io_threads.join().unwrap();
            Ok(())
        });
        self.connections.insert(key, (thread, conn_shutdown));
        return key;
    }
}

impl<I> ServerConnection<I>
where
    I: InitializeConnectionMessage,
{
    /// Given an address, return an iterator over incoming server connections
    /// Initialize the connection. Sends the I::Response
    /// to the client and returns I as a Request on success.
    /// If more fine-grained initialization is required use
    /// `initialize_start`/`initialize_finish`.
    pub fn initialize(&self, response: I::Response) -> Result<Request, Error> {
        let init_req = self.initialize_start()?;
        tracing::debug!("server got init req");

        self.initialize_finish(response)?;

        tracing::debug!("server finished initialization");

        Ok(init_req)
    }

    /// Starts the initialization process by waiting for an initialize
    /// request from the client. Use this for more advanced customization than
    /// `initialize` can provide.
    ///
    /// Returns the `I` type as Request
    pub fn initialize_start(&self) -> Result<Request, Error> {
        self.initialize_start_while(|| true)
    }

    /// Starts the initialization process by waiting for an initialize as described in
    /// [`Self::initialize_start`] as long as `running` returns
    /// `true` while the return value can be changed through a sig handler such as `CTRL + C`.
    pub fn initialize_start_while<C>(&self, running: C) -> Result<Request, Error>
    where
        C: Fn() -> bool,
    {
        while running() {
            let msg = match self
                .conn
                .receiver
                .recv_timeout(std::time::Duration::from_secs(1))
            {
                Ok(msg) => msg,
                Err(RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return Err(ErrorKind::Disconnect.into()),
            };

            // println!("message: {msg:#?}");
            match msg {
                Message::Req(req) if I::matches(&req) => return Ok(req),
                // Respond to non-initialize requests with ServerNotInitialized
                Message::Req(ref req) => {
                    let err: Error = ErrorKind::uninitialized(&msg).into();
                    let resp = Response::from_error(req.id.clone(), err);
                    self.conn.sender.send(resp.into()).unwrap();
                    continue;
                }
                _ => {
                    return Err(ErrorKind::other(
                        &format!("expected initialize request, got {msg:?}"),
                        ErrorCode::ServerErrorStart,
                    )
                    .into());
                }
            }
        }

        Err(ErrorKind::other(
            &format!("Initialization has been aborted during initialization",),
            ErrorCode::ServerErrorStart,
        )
        .into())
    }

    /// Finishes the initialization process by sending an `InitializeResult` to the client
    pub fn initialize_finish(&self, init_res: I::Response) -> Result<(), Error> {
        let resp = I::init_response(init_res);
        self.conn.sender.send(resp.into()).map_err(|e| {
            ErrorKind::other(
                &format!("server failed to send initialization response: {e:#?}"),
                ErrorCode::InternalError,
            )
            .into()
        })?;
        tracing::debug!("server sent initialization response");
        Ok(())
    }

    /// Finishes the initialization process as described in [`Self::initialize_finish`] as
    /// long as `running` returns `true` while the return value can be changed through a sig
    /// handler such as `CTRL + C`.
    pub fn initialize_finish_while<C>(
        &self,
        initialize_response: Response,
        running: C,
    ) -> Result<(), Error>
    where
        C: Fn() -> bool,
    {
        // let resp = initialize_response.into_response(initialize_id);
        self.conn.sender.send(initialize_response.into()).unwrap();

        while running() {
            let msg = match self
                .conn
                .receiver
                .recv_timeout(std::time::Duration::from_secs(1))
            {
                Ok(msg) => msg,
                Err(RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(ErrorKind::Disconnect.into());
                }
            };

            match msg {
                Message::Res(res) if I::Response::try_from_response(&res).is_ok() => {
                    return Ok(());
                }
                msg => {
                    return Err(ErrorKind::other(
                        &format!(r#"expected Initialize response, got: {msg:?}"#),
                        ErrorCode::ServerErrorStart,
                    )
                    .into());
                }
            }
        }

        Err(ErrorKind::other(
            &String::from("Initialization has been aborted during initialization"),
            ErrorCode::ServerErrorStart,
        )
        .into())
    }

    pub fn initialize_while<C>(
        &self,
        response_payload: Option<serde_json::Value>,
        running: C,
    ) -> Result<Request, Error>
    where
        C: Fn() -> bool,
    {
        let init_req = self.initialize_start_while(&running)?;

        let id = init_req.id.to_string();
        let res = I::Response::try_from_response(&Response::new_ok(&id, response_payload))
            .unwrap()?
            .into_response(id)
            .expect("failed to get response");

        self.initialize_finish_while(res, running)?;

        Ok(init_req)
    }
}
