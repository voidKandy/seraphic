#[cfg(feature = "client")]
pub mod client;
pub mod connection;
pub mod error;
pub mod io;
pub mod msg;
#[cfg(feature = "server")]
pub mod server;
use error::Error;
pub use msg::{Message, MessageId, Request, Response};
pub use seraphic_derive as derive;

type MainErr = Box<dyn std::error::Error + Send + Sync + 'static>;
type MainResult<T> = std::result::Result<T, MainErr>;
pub const JSONRPC_FIELD: &str = "2.0";
pub trait RpcNamespace: PartialEq + Copy {
    const SEPARATOR: &str;
    fn as_str(&self) -> &str;
    fn try_from_str(str: &str) -> Option<Self>
    where
        Self: Sized;
}

pub trait RpcResponse:
    std::fmt::Debug + Clone + serde::Serialize + for<'de> serde::Deserialize<'de>
{
    fn try_from_response(res: &Response) -> MainResult<Result<Self, Error>> {
        if let Some(e) = &res.error {
            return Ok(Err(e.clone()));
        }
        let val = res
            .result
            .as_ref()
            .ok_or(std::io::Error::other("No result or error in response"))?;

        let me: Self = serde_json::from_value(val.clone()).expect("failed to deserialize Response");

        Ok(Ok(me))
    }

    /// Only fails if self fails to serialize
    fn into_response(&self, id: impl ToString) -> MainResult<Response> {
        let result = serde_json::to_value(self)?;
        Ok(Response {
            jsonrpc: JSONRPC_FIELD.to_string(),
            id: id.to_string(),
            result: Some(result),
            error: None,
        })
    }
}

pub trait RpcRequest:
    std::fmt::Debug + Clone + serde::Serialize + for<'de> serde::Deserialize<'de>
{
    type Response: RpcResponse;
    type Namespace: RpcNamespace;
    fn method() -> &'static str;
    fn namespace() -> Self::Namespace;
    /// Only fails if self fails to serialize
    fn into_request(&self, id: impl ToString) -> MainResult<Request> {
        let params = serde_json::to_value(&self)?;
        let method = format!("{}_{}", Self::namespace().as_str(), Self::method());
        Ok(Request {
            jsonrpc: JSONRPC_FIELD.to_string(),
            method,
            params,
            id: id.to_string(),
        })
    }
    fn try_from_request(req: &Request) -> MainResult<Option<Self>> {
        if let Some((namespace_str, method_str)) = req.method.split_once(Self::Namespace::SEPARATOR)
        {
            let namespace = Self::Namespace::try_from_str(namespace_str).unwrap();
            if namespace != Self::namespace() || method_str != Self::method() {
                return Ok(None);
            }

            return Self::try_from_json(&req.params).and_then(|me| Ok(Some(me)));
        }
        Ok(None)
    }
    fn try_from_json(json: &serde_json::Value) -> MainResult<Self>
    where
        Self: Sized;
}

pub enum MsgWrapper<Req, Res> {
    Req { id: MessageId, req: Req },
    Res { id: MessageId, res: Res },
    Shutdown,
}

impl<Req, Res> TryFrom<msg::Message> for MsgWrapper<Req, Res>
where
    Req: RequestWrapper,
    Res: ResponseWrapper,
{
    type Error = MainErr;
    fn try_from(value: msg::Message) -> Result<Self, Self::Error> {
        match value {
            msg::Message::Req(req) => Ok(Self::Req {
                id: req.id.clone(),
                req: Req::try_from_req(req)?,
            }),
            msg::Message::Res(res) => Ok(Self::Res {
                id: res.id.clone(),
                res: Res::try_from_res(res)?,
            }),
            msg::Message::Shutdown => Ok(Self::Shutdown),
        }
    }
}

impl<Req, Res> MsgWrapper<Req, Res>
where
    Req: RequestWrapper,
    Res: ResponseWrapper,
{
    pub fn id(&self) -> Option<&MessageId> {
        match self {
            Self::Req { id, .. } | Self::Res { id, .. } => Some(id),
            _ => None,
        }
    }
    pub fn as_req(&self) -> Option<(&MessageId, &Req)> {
        match self {
            Self::Req { id, req } => Some((id, req)),
            _ => None,
        }
    }
    pub fn as_res(&self) -> Option<(&MessageId, &Res)> {
        match self {
            Self::Res { id, res } => Some((id, res)),
            _ => None,
        }
    }
}

pub trait ResponseWrapper: std::fmt::Debug {
    fn into_res(self, id: impl ToString) -> Response
    where
        Self: Sized;
    fn try_from_res(res: Response) -> MainResult<Self>
    where
        Self: Sized;
}

pub trait RequestWrapper: std::fmt::Debug {
    fn into_req(self, id: impl ToString) -> Request
    where
        Self: Sized;
    fn try_from_req(req: Request) -> MainResult<Self>
    where
        Self: Sized;
}
