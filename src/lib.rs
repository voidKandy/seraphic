pub mod error;
pub mod msg;
pub mod packet;
#[cfg(feature = "tokio")]
pub mod tokio;

use std::any::TypeId;

use error::Error;
pub use msg::{Message, MessageId, Request, Response};
pub use seraphic_derive as derive;
use serde_json::Value;

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
    std::fmt::Debug + Clone + serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq
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
    std::fmt::Debug
    + Clone
    + serde::Serialize
    + for<'de> serde::Deserialize<'de>
    + std::marker::Send
    + 'static
    + PartialEq
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

pub trait ResponseWrapper: std::fmt::Debug + PartialEq {
    fn into_message<Rq>(self, id: impl ToString) -> Message<Rq, Self>
    where
        Rq: RequestWrapper,
        Self: Sized,
    {
        Message::Res {
            id: id.to_string(),
            res: self,
        }
    }
    fn into_res(&self, id: impl ToString) -> Response
    where
        Self: Sized;
    fn try_from_res(res: Response) -> MainResult<Result<Self, Error>>
    where
        Self: Sized;
}

pub trait RequestWrapper: std::fmt::Debug + PartialEq {
    fn into_message<Rs>(self, id: impl ToString) -> Message<Self, Rs>
    where
        Rs: ResponseWrapper,
        Self: Sized,
    {
        Message::Req {
            id: id.to_string(),
            req: self,
        }
    }

    fn into_req(&self, id: impl ToString) -> Request
    where
        Self: Sized;
    fn try_from_req(req: Request) -> MainResult<Self>
    where
        Self: Sized;
}
