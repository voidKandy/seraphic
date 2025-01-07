use std::marker::PhantomData;

use crossbeam_channel::{RecvError, RecvTimeoutError};

use crate::{
    connection::{Connection, InitializeConnectionMessage},
    error::{Error, ErrorCode, ErrorKind},
    io::IoThreads,
    MainResult, Message, Request, Response, RpcRequest, RpcResponse,
};

pub struct Server<I> {
    pub conn: Connection<I>,
    pub threads: IoThreads,
}

impl<I> From<(Connection<I>, IoThreads)> for Server<I> {
    fn from((conn, threads): (Connection<I>, IoThreads)) -> Self {
        Self { conn, threads }
    }
}

impl<I> Server<I>
where
    I: InitializeConnectionMessage,
{
    /// Initialize the connection. Sends the I::Response
    /// to the client and returns I as a Request on success.
    /// If more fine-grained initialization is required use
    /// `initialize_start`/`initialize_finish`.
    pub fn initialize(&self, response: I::Response) -> Result<Request, Error> {
        let init_req = self.initialize_start()?;

        self.initialize_finish(response)?;

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

            println!("message: {msg:#?}");
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
        self.conn.sender.send(resp.into()).unwrap();
        match &self.conn.receiver.recv() {
            Ok(Message::Res(res)) if I::Response::try_from_response(&res).is_ok() => Ok(()),
            Ok(msg) => Err(ErrorKind::other(
                &format!(r#"expected Message::Open, got: {msg:?}"#),
                ErrorCode::ServerErrorStart,
            )
            .into()),
            Err(RecvError) => Err(ErrorKind::Disconnect.into()),
        }
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
