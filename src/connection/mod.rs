mod io;
use crate::{
    error::{Error, ErrorCode, ErrorKind},
    msg::{Message, Request, Response},
    RpcRequest, RpcResponse,
};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use io::{make_reader, make_writer, IoThreads};
use std::{
    marker::PhantomData,
    net::TcpStream,
    sync::{atomic::AtomicBool, Arc},
};

/// Connection is just a pair of channels of LSP messages.
/// Generic I is an RpcRequest for the initial request
pub struct Connection<I> {
    pub sender: Sender<Message>,
    pub receiver: Receiver<Message>,
    pub io_threads: IoThreads,
    init_request_marker: PhantomData<I>,
}

enum ReqOrRes<'a> {
    Req(&'a Request),
    Res(&'a Response),
}

impl<'a> From<&'a Request> for ReqOrRes<'a> {
    fn from(value: &'a Request) -> Self {
        Self::Req(value)
    }
}
impl<'a> From<&'a Response> for ReqOrRes<'a> {
    fn from(value: &'a Response) -> Self {
        Self::Res(value)
    }
}

#[allow(private_bounds)]
pub trait InitializeConnectionMessage: RpcRequest {
    const ID: &str;

    fn matches<'a>(into: impl Into<ReqOrRes<'a>>) -> bool {
        match Into::<ReqOrRes<'a>>::into(into) {
            ReqOrRes::Res(r) => <Self as RpcRequest>::Response::try_from_response(r).is_ok(),
            ReqOrRes::Req(r) => <Self as RpcRequest>::try_from_request(r).is_ok(),
        }
    }

    fn init_response(res: <Self as RpcRequest>::Response) -> Response {
        res.into_response(Self::ID).expect("failed to get response")
    }

    fn init_request(&self) -> Request {
        self.into_request(Self::ID).unwrap()
    }
}

impl<I> Connection<I>
where
    I: InitializeConnectionMessage,
{
    pub(crate) fn socket_transport(stream: TcpStream) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let (reader_receiver, reader) =
            make_reader::<I>(stream.try_clone().unwrap(), Arc::clone(&shutdown));
        let (writer_sender, writer) = make_writer(stream, Arc::clone(&shutdown));
        let io_threads = IoThreads {
            reader,
            writer,
            shutdown_signal: shutdown,
        };
        Self {
            sender: writer_sender,
            receiver: reader_receiver,
            io_threads,
            init_request_marker: PhantomData,
        }
    }

    /// If `message` is not `Shutdown` returns false`
    /// Message::Shutdown(false): responds with Message::Shutdown(true) waits for an expected Message::Exit
    /// Message::Shutdown(true): responds with Message::Exit and returns
    pub fn handle_shutdown(&self, message: &Message) -> Result<bool, Error> {
        match message {
            Message::Shutdown(true) => {
                self.sender.send(Message::Exit).map_err(|err| {
                    ErrorKind::other(
                        &format!("failed to send exit message: {err:?}"),
                        ErrorCode::InternalError,
                    )
                    .into()
                })?;
                return Ok(true);
            }
            Message::Shutdown(false) => {
                self.sender.send(Message::Shutdown(true)).map_err(|err| {
                    ErrorKind::other(
                        &format!("failed to send exit message: {err:?}"),
                        ErrorCode::InternalError,
                    )
                    .into()
                })?;
            }
            _ => return Ok(false),
        };

        tracing::debug!("waiting for exit");
        match &self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(30))
        {
            Ok(Message::Exit) => {
                tracing::debug!("received exit ");
                Ok(true)
            }
            Ok(msg) => Err(ErrorKind::other(
                &format!("unexpected message during shutdown: {msg:?}"),
                ErrorCode::ServerErrorEnd,
            )
            .into()),
            Err(RecvTimeoutError::Timeout) => Err(ErrorKind::other(
                "timed out waiting for exit notification",
                ErrorCode::ServerErrorEnd,
            )
            .into()),
            Err(RecvTimeoutError::Disconnected) => Err(ErrorKind::other(
                "channel disconnected waiting for exit notification",
                ErrorCode::ServerErrorEnd,
            )
            .into()),
        }
    }
}
