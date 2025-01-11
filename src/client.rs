use crate::{
    connection::{Connection, InitializeConnectionMessage},
    error::{ErrorCode, ErrorKind},
    MainResult, Response, RpcRequest, RpcResponse,
};
use std::net::{TcpStream, ToSocketAddrs};

pub struct ClientConnection<I> {
    pub conn: Connection<I>,
}

impl<I> ClientConnection<I>
where
    I: InitializeConnectionMessage,
{
    pub fn connect(addr: impl ToSocketAddrs) -> std::io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        let conn = Connection::<I>::socket_transport(stream);
        Ok(Self { conn })
    }

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
