pub mod async_io;
pub mod serde_;
pub mod sync_io;
use seraphic::{
    derive::{RequestWrapper, ResponseWrapper, RpcNamespace, RpcRequest},
    packet::TcpPacket,
    ResponseWrapper, RpcNamespace, RpcRequest, RpcResponse,
};
use serde::{Deserialize, Serialize};

#[derive(RpcNamespace, Clone, Copy, PartialEq, Eq)]
pub enum TestNS {
    Test,
}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug, PartialEq)]
#[rpc_request(namespace = "TestNS:test")]
pub struct TestRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestResponse {}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug, PartialEq)]
#[rpc_request(namespace = "TestNS:test")]
pub struct FooRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FooResponse {}

#[derive(Debug, Clone, RequestWrapper, PartialEq)]
pub enum MyRequest {
    Test(TestRequest),
    Foo(FooRequest),
}

#[derive(Debug, Clone, ResponseWrapper, PartialEq)]
pub enum MyResponse {
    Test(TestResponse),
    Foo(FooResponse),
}

pub type Message = seraphic::Message<MyRequest, MyResponse>;
pub type MessagePacket = TcpPacket<Message>;
