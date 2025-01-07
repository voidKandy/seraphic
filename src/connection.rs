use crate::{
    error::{Error, ErrorCode, ErrorKind},
    io::IoThreads,
    msg::{Message, MessageId, Request, Response},
    RpcNamespace, RpcRequest, RpcResponse,
};
use crossbeam_channel::{bounded, Receiver, RecvError, RecvTimeoutError, Sender};
use seraphic_derive::{RpcNamespace, RpcRequest};
use serde::{Deserialize, Serialize};
use std::{
    marker::PhantomData,
    net::{TcpListener, TcpStream, ToSocketAddrs},
};

/// Connection is just a pair of channels of LSP messages.
pub struct Connection {
    pub sender: Sender<Message>,
    pub receiver: Receiver<Message>,
}

/// Generic I is an RpcRequest for the initial request
impl Connection {
    /// Create connection over standard in/standard out.
    ///
    /// Use this to create a real language server.
    pub fn stdio() -> (Connection, IoThreads) {
        let (sender, receiver, io_threads) = crate::io::stdio_transport();
        (Connection { sender, receiver }, io_threads)
    }

    /// Open a connection over tcp.
    /// This call blocks until a connection is established.
    ///
    /// Use this to create a real language server.
    pub fn connect<A: ToSocketAddrs>(addr: A) -> std::io::Result<(Connection, IoThreads)> {
        let stream = TcpStream::connect(addr)?;
        let (sender, receiver, io_threads) = crate::io::socket_transport(stream);
        Ok((Connection { sender, receiver }, io_threads))
    }

    /// Listen for a connection over tcp.
    /// This call blocks until a connection is established.
    ///
    /// Use this to create a real language server.
    pub fn listen<A: ToSocketAddrs>(addr: A) -> std::io::Result<(Connection, IoThreads)> {
        let listener = TcpListener::bind(addr)?;
        let (stream, _) = listener.accept()?;
        let (sender, receiver, io_threads) = crate::io::socket_transport(stream);
        Ok((Connection { sender, receiver }, io_threads))
    }

    /// Creates a pair of connected connections.
    ///
    /// Use this for testing.
    pub fn memory() -> (Connection, Connection) {
        let (s1, r1) = crossbeam_channel::unbounded();
        let (s2, r2) = crossbeam_channel::unbounded();
        (
            Connection {
                sender: s1,
                receiver: r2,
            },
            Connection {
                sender: s2,
                receiver: r1,
            },
        )
    }

    pub fn initialize_start<I: RpcRequest>(
        &self,
    ) -> Result<(MessageId, Option<serde_json::Value>), Error> {
        self.initialize_start_while(|| true)
    }

    pub fn initialize_start_while<C, I: RpcRequest>(
        &self,
        running: C,
    ) -> Result<(MessageId, Option<serde_json::Value>), Error>
    where
        C: Fn() -> bool,
    {
        while running() {
            let msg = match self
                .receiver
                .recv_timeout(std::time::Duration::from_secs(1))
            {
                Ok(msg) => msg,
                Err(RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return Err(ErrorKind::Disconnect.into()),
            };

            match msg {
                Message::Req(req) if I::try_from_request(&req).is_ok() => return Ok(msg.to_send()),
                // Respond to non-initialize requests with ServerNotInitialized
                Message::Req(req) => {
                    let err: Error = ErrorKind::Uninitialized(&msg).into();
                    let resp = Response::from_error(req.id, err);
                    self.sender.send(resp.into()).unwrap();
                    continue;
                }
                Message::Res(n) => {
                    continue;
                }
                msg => {
                    return Err(ErrorKind::other(
                        &format!("expected initialize request, got {msg:?}"),
                        ErrorCode::ServerErrorStart,
                    ));
                }
            };
        }

        Err(ErrorKind::other(
            &format!("Initialization has been aborted during initialization",),
            ErrorCode::ServerErrorStart,
        ));
    }

    /// Finishes the initialization process by sending an `InitializeResult` to the client
    pub fn initialize_finish<I: RpcRequest>(
        &self,
        initialize_id: MessageId,
        init_req: I,
        // initialize_result: Option<serde_json::Value>,
    ) -> Result<(), Error> {
        // let resp = Response::new_ok(initialize_id, initialize_result);
        let req = init_req.into_rpc_request(initialize_id);
        self.sender.send(init_req).unwrap();
        match &self.receiver.recv() {
            Ok(Message::Res(res)) if I::Response::try_from_response(&res).is_ok() => Ok(()),
            Ok(msg) => Err(ErrorKind::other(
                &format!(r#"expected Message::Open, got: {msg:?}"#),
                ErrorCode::ServerErrorStart,
            )),
            Err(RecvError) => Err(ErrorKind::Disconnect.into()),
        }
    }

    /// Finishes the initialization process as described in [`Self::initialize_finish`] as
    /// long as `running` returns `true` while the return value can be changed through a sig
    /// handler such as `CTRL + C`.
    pub fn initialize_finish_while<C, I: RpcRequest>(
        &self,
        initialize_response: Response,
        running: C,
    ) -> Result<(), Error>
    where
        C: Fn() -> bool,
    {
        // let resp = initialize_result.into_response(initialize_id);
        self.sender.send(initialize_response).unwrap();

        while running() {
            let msg = match self
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
                    ));
                }
            }
        }

        Err(ErrorKind::other(
            &String::from("Initialization has been aborted during initialization"),
            ErrorCode::ServerErrorStart,
        ))
    }
    pub fn initialize<I: RpcRequest>(&self, init_req: I) -> Result<(), Error> {
        let (id, params) = self.initialize_start()?;

        self.initialize_finish(id, init_req)?;

        Ok(params)
    }

    pub fn initialize_while<C, I: RpcRequest>(
        &self,
        // init_req: I::Response,
        response_payload: Option<serde_json::Value>,
        running: C,
    ) -> Result<I::Response, Error>
    where
        C: Fn() -> bool,
    {
        let (id, init_res) = self.initialize_start_while(&running)?;

        let res = I::Response::try_from_response(Response::new_ok(id, response_payload));

        self.initialize_finish_while(init_res, running)?;

        Ok(init_res)
    }

    /// If `req` is `Shutdown`, respond to it and return `true`, otherwise return `false`
    pub fn handle_shutdown(&self, message: &Message) -> Result<bool, Error> {
        if !matches!(message, Message::Close) {
            return Ok(false);
        }
        let resp = Response::new_ok(Message::Close.id(), None);
        let _ = self.sender.send(resp.into());
        match &self
            .receiver
            .recv_timeout(std::time::Duration::from_secs(30))
        {
            Ok(Message::Close) => (),
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
        derive::{RpcNamespace, RpcRequest},
        msg::Response,
        RpcNamespace, RpcRequest, RpcResponse,
    };
    use crossbeam_channel::unbounded;
    use serde::{Deserialize, Serialize};
    use serde_json::to_value;

    use crate::msg::MessageId;

    use super::{Connection, Error, Message};

    struct TestCase {
        test_messages: Vec<Message>,
        expected_resp: Result<(MessageId, serde_json::Value), Error>,
    }

    #[derive(RpcNamespace, Clone, Copy, PartialEq, Eq)]
    enum TestNS {
        Test,
    }

    #[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
    #[rpc_request(namespace = "TestNS:test")]
    struct TestInitRequest {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestInitResponse {}

    fn initialize_start_test(test_case: TestCase) {
        let (reader_sender, reader_receiver) = unbounded::<Message>();
        let (writer_sender, writer_receiver) = unbounded::<Message>();
        let conn = Connection {
            sender: writer_sender,
            receiver: reader_receiver,
        };

        for msg in test_case.test_messages {
            assert!(reader_sender.send(msg).is_ok());
        }

        let resp = conn.initialize_start::<TestInitRequest>();
        assert_eq!(test_case.expected_resp, resp);

        assert!(writer_receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .is_err());
    }

    #[test]
    fn not_exit_notification() {
        let response = TestInitResponse {};

        // let params_as_value = to_value(InitializeParams::default()).unwrap();
        // let req_id = RequestId::from(234);
        // let request = crate::Request {
        //     id: req_id.clone(),
        //     method: Initialize::METHOD.to_owned(),
        //     params: params_as_value.clone(),
        // };

        initialize_start_test(TestCase {
            test_messages: vec![notification.into(), request.into()],
            expected_resp: Ok((req_id, params_as_value)),
        });
    }

    #[test]
    fn exit_notification() {
        let notification = crate::Notification {
            method: Exit::METHOD.to_owned(),
            params: to_value(()).unwrap(),
        };
        let notification_msg = Message::from(notification);

        initialize_start_test(TestCase {
            test_messages: vec![notification_msg.clone()],
            expected_resp: Err(ProtocolError::new(format!(
                "expected initialize request, got {notification_msg:?}"
            ))),
        });
    }
}
