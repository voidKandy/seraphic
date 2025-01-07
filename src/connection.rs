use crate::{
    error::{Error, ErrorCode, ErrorKind},
    io::IoThreads,
    msg::{Message, Request, Response},
    RpcRequest, RpcResponse,
};
use crossbeam_channel::{Receiver, RecvError, RecvTimeoutError, Sender};
use std::{
    marker::PhantomData,
    net::{TcpListener, TcpStream, ToSocketAddrs},
};

/// Connection is just a pair of channels of LSP messages.
pub struct Connection<I> {
    pub sender: Sender<Message>,
    pub receiver: Receiver<Message>,
    /// Generic I is an RpcRequest for the initial request
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
    /// Create connection over standard in/standard out.
    ///
    /// Use this to create a real language server.
    pub fn stdio() -> (Connection<I>, IoThreads) {
        let (sender, receiver, io_threads) = crate::io::stdio_transport::<I>();
        (
            Connection {
                sender,
                receiver,
                init_request_marker: PhantomData,
            },
            io_threads,
        )
    }

    /// Open a connection over tcp.
    /// This call blocks until a connection is established.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> std::io::Result<(Connection<I>, IoThreads)> {
        let stream = TcpStream::connect(addr)?;
        let (sender, receiver, io_threads) = crate::io::socket_transport::<I>(stream);
        Ok((
            Connection {
                sender,
                receiver,
                init_request_marker: PhantomData,
            },
            io_threads,
        ))
    }

    /// Listen for a connection over tcp.
    /// This call blocks until a connection is established.
    pub fn listen<A: ToSocketAddrs>(addr: A) -> std::io::Result<(Connection<I>, IoThreads)> {
        let listener = TcpListener::bind(addr)?;
        let (stream, _addr) = listener.accept()?;
        let (sender, receiver, io_threads) = crate::io::socket_transport::<I>(stream);
        Ok((
            Connection {
                sender,
                receiver,
                init_request_marker: PhantomData,
            },
            io_threads,
        ))
    }

    /// Creates a pair of connected connections.
    ///
    /// Use this for testing.
    // pub fn memory() -> (Connection<I>, Connection<I>) {
    //     let (s1, r1) = crossbeam_channel::unbounded();
    //     let (s2, r2) = crossbeam_channel::unbounded();
    //     (
    //         Connection::<I> {
    //             sender: s1,
    //             receiver: r2,
    //             init_request_marker: PhantomData,
    //         },
    //         Connection::<I> {
    //             sender: s2,
    //             receiver: r1,
    //             init_request_marker: PhantomData,
    //         },
    //     )
    // }

    /// If `req` is `Shutdown`, respond to it and return `true`, otherwise return `false`
    pub fn handle_shutdown(&self, message: &Message) -> Result<bool, Error> {
        if !matches!(message, Message::Shutdown) {
            return Ok(false);
        }
        let resp = Response::new_ok(Message::Shutdown.id(), None);
        let _ = self.sender.send(resp.into());
        match &self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(30))
        {
            Ok(Message::Shutdown) => (),
            Ok(msg) => {
                return Err(ErrorKind::other(
                    &format!("unexpected message during shutdown: {msg:?}"),
                    ErrorCode::ServerErrorEnd,
                )
                .into())
            }
            Err(RecvTimeoutError::Timeout) => {
                return Err(ErrorKind::other(
                    "timed out waiting for exit notification",
                    ErrorCode::ServerErrorEnd,
                )
                .into())
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ErrorKind::other(
                    "channel disconnected waiting for exit notification",
                    ErrorCode::ServerErrorEnd,
                )
                .into())
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        client::Client,
        derive::{RpcNamespace, RpcRequest},
        error::{ErrorCode, ErrorKind},
        msg::{Request, Response},
        server::Server,
        RpcNamespace, RpcRequest, RpcResponse,
    };
    use crossbeam_channel::unbounded;
    use serde::{Deserialize, Serialize};
    use serde_json::to_value;

    use crate::msg::MessageId;

    use super::{Connection, Error, InitializeConnectionMessage, Message};

    struct TestCase {
        test_messages: Vec<Message>,
        expected_msg: Result<Message, Error>,
    }

    // fn initialize_start_test(test_case: TestCase) {
    //     let client = Client::from(Connection::<TestInitRequest>::stdio());
    //     let server = Server::from(Connection::<TestInitRequest>::stdio());

    // let (reader_sender, reader_receiver) = unbounded::<Message>();
    // let (writer_sender, writer_receiver) = unbounded::<Message>();
    // let conn = Connection::<TestInitRequest> {
    //     sender: writer_sender,
    //     receiver: reader_receiver,
    //     init_request_marker: std::marker::PhantomData,
    // };

    // assert!(client.conn.sen(msg).is_ok());
    // for msg in test_case.test_messages {
    //     }
    //
    //     let resp = conn.initialize_start();
    //     assert_eq!(test_case.expected_msg, resp.and_then(|r| Ok(r.into())));
    //
    //     assert!(writer_receiver
    //         .recv_timeout(std::time::Duration::from_secs(1))
    //         .is_err());
    // }

    // #[test]
    // fn () {
    //     let response = TestInitRequest::response(TestInitResponse {});
    //     let request = TestInitRequest {}.request();
    //
    //     initialize_start_test(TestCase {
    //         test_messages: vec![request.clone().into(), response.into()],
    //         expected_msg: Ok(request.into()),
    //     });
    // }
    //
    // #[test]
    // fn exit_notification() {
    //     let resp = TestInitRequest::response(TestInitResponse {});
    //     let msg: Message = resp.into();
    //     let expected_err: Error = ErrorKind::other(
    //         &format!("expected initialize request, got {msg:?}"),
    //         ErrorCode::ServerErrorStart,
    //     )
    //     .into();
    //
    //     initialize_start_test(TestCase {
    //         test_messages: vec![msg.clone()],
    //         expected_msg: Err(expected_err),
    //     });
    // }
}
