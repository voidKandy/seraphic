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

seraphic_derive::wrapper!(ResponseWrapper, MyResponse, [TestResponse]);
seraphic_derive::wrapper!(RequestWrapper, MyRequest, [TestRequest]);

pub type Message = seraphic::Message<MyRequest, MyResponse>;
pub type MessagePacket = TcpPacket<Message>;
