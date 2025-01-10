use std::marker::PhantomData;

use crossbeam_channel::RecvError;

use crate::{
    connection::{Connection, InitializeConnectionMessage},
    error::{ErrorCode, ErrorKind},
    io::IoThreads,
    MainResult, Response, RpcRequest, RpcResponse,
};

pub struct Client<I> {
    pub conn: Connection<I>,
    pub threads: IoThreads,
}

impl<I> From<(Connection<I>, IoThreads)> for Client<I> {
    fn from((conn, threads): (Connection<I>, IoThreads)) -> Self {
        Self { conn, threads }
    }
}

impl<I> Client<I>
where
    I: InitializeConnectionMessage,
{
    /// Initializes the connection with a server by sending an initialization request
    /// Hangs until an inialization response is returned
    /// Outer Errors if serialization fails
    /// Inner Errors if an unexpected response is returned
    pub fn initialize(&self, req: I) -> MainResult<Result<Response, crate::error::Error>> {
        self.conn
            .sender
            .send(req.init_request().into())
            .expect("failed to send");

        match self.conn.receiver.recv() {
            Ok(msg) => match msg {
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
            },
            Err(err) => {
                return Err(std::io::Error::other(format!("receive error: {err:#?}")).into())
            }
        }
    }
}
