pub mod error;
pub mod msg;
pub mod packet;
#[cfg(feature = "tokio")]
pub mod tokio;

use error::Error;
pub use msg::{IdentifiedResponse, Message, MessageId, Request, Response};
pub use seraphic_derive as derive;
use serde_json::json;

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
    const IDENTITY: &str;

    fn try_from_response(res: &IdentifiedResponse) -> MainResult<Result<Self, Error>> {
        if res.id.as_str() != Self::IDENTITY {
            return Err(std::io::Error::other(format!(
                "Identities do not match, expected: {} got: {}",
                Self::IDENTITY,
                res.id
            ))
            .into());
        }
        if let Some(e) = &res.res.error {
            return Ok(Err(e.clone()));
        }
        let empty_json = json!({});
        let val = res.res.result.as_ref().unwrap_or(&empty_json);

        let me: Self = serde_json::from_value(val.clone()).expect("failed to deserialize Response");

        Ok(Ok(me))
    }

    /// Only fails if self fails to serialize
    fn into_response(&self, id: impl ToString) -> MainResult<IdentifiedResponse> {
        let result = serde_json::to_value(self)?;
        let res = Response {
            jsonrpc: JSONRPC_FIELD.to_string(),
            id: id.to_string(),
            result: Some(result),
            error: None,
        };
        Ok(IdentifiedResponse {
            id: Self::IDENTITY.to_string(),
            res,
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

    fn namespace_method() -> String {
        format!(
            "{}{}{}",
            Self::namespace().as_str(),
            Self::Namespace::SEPARATOR,
            Self::method()
        )
    }

    /// Only fails if self fails to serialize
    fn into_request(&self, id: impl ToString) -> MainResult<Request> {
        let params = serde_json::to_value(&self)?;
        Ok(Request {
            jsonrpc: JSONRPC_FIELD.to_string(),
            method: Self::namespace_method(),
            params,
            id: id.to_string(),
        })
    }
    fn try_from_request(req: &Request) -> MainResult<Self> {
        if let Some((namespace_str, method_str)) = req.method.split_once(Self::Namespace::SEPARATOR)
        {
            let namespace = Self::Namespace::try_from_str(namespace_str).unwrap();
            if namespace != Self::namespace() || method_str != Self::method() {
                return Err(std::io::Error::other(format!("namespace & method do not match expected. Got namespace: {namespace_str} with method: {method_str} expected namespace: {} with method: {}",
                    Self::namespace().as_str(), Self::method()
                )).into());
            }

            return Self::try_from_json(&req.params);
        }
        Err(std::io::Error::other(format!(
            "Request method: {} could not be split by separator: {}",
            req.method,
            Self::Namespace::SEPARATOR
        ))
        .into())
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
    fn into_res(&self, id: impl ToString) -> IdentifiedResponse
    where
        Self: Sized;
    fn try_from_res(res: IdentifiedResponse) -> MainResult<Result<Self, Error>>
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
