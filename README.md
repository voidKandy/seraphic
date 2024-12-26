<div align="center">
  <h1>seraphic</h1>
</div>
<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/seraphic">
    <img src="https://img.shields.io/crates/v/seraphic.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/seraphic">
    <img src="https://img.shields.io/crates/d/seraphic.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs -->
  <a href="https://docs.rs/seraphic">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
</div>


A super light JSON RPC 2.0 implementation.

> **_WARNING_**:
This is very early in development and is subject to significant change.


## Creating a server
As of right now, `seraphic` only handles the creation of servers. Clients can be created any way you choose, so long as you dial the correct address and send messages compliant with the [JSON RPC 2.0 specification](https://www.jsonrpc.org/specification).

### `RpcListeningThread`
```rust
pub struct RpcListeningThread {
    pub recv: tokio::sync::mpsc::Receiver<Request>,
    pub sender: tokio::sync::mpsc::Sender<Response>,
    _thread: JoinHandle<()>,
}
```
This is the main struct for handling all server operations, one can be created with `RpcListeningThread::new`. Requests can be polled from `recv` , and responses can be sent back through `sender`.
```rust
let server_thread = RpcListeningThread::new("127.0.0.1:3000")?;
if let Some(req) = server_thread.recv.recv().await {
    // Do some work to get response
    server_thread.sender.send(response).await?;
}
```

## Important traits
Sending JSON through a server is easy enough, but what's really helpful about `seraphic` is the way it abstracts Request Methods, expected Responses, and errors. These are the traits used to facilitate this abstraction:
+ `RpcNamespace` - Facilitates the management of method namespaces.
+ `RpcRequest` - Defines the namespace/method a request is associated with & facilitates serialization to/from the `socket::Request` struct.
+ `RpcRequestWrapper` - a wrapper struct meant to contain all requests your server accepts
+ `RpcResponse` - Simply a marker trait for marking a struct as what you expect to be returned from the successful processing of a request.
The best thing about all these traits is that they each have a derive implementation for minimal boilerplate!
+ `RpcHandler` - to be implemented on whatever you are using to process requests to return responses

## Example
```rust
// This will define the namespaces "foo", "bar", and "baz"
#[derive(RpcNamespace)]
enum MyNamespace {
    Foo,
    Bar,
    Baz,
}

// The rpc_request derive attribute *requires* you pass a namespace argument, which is formatted as "<Namespace Struct Name>:<variant>"
// The RpcRequest derive macro expects the struct it is derived on to end in the suffix 'Request', and for there to be another struct with the same prefix, but with 'Response' as the suffix.
// RpcRequest's Derive macro will expand to implement RpcResponse on it's associated response struct
#[derive(RpcRequest, Debug, Clone, Serialize, Deserialize)]
#[rpc_request(namespace = "MyNamespace:bar")]
struct SomeBarRequest {
    param1: String,
    param2: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SomeBarResponse {
    value1: u32,
    value2: String,
}

//  If you wish to use a struct by a different name for your expected response object, you can pass it in the rpc_request attribute.
#[derive(RpcRequest, Debug, Clone, Serialize, Deserialize)]
#[rpc_request(namespace = "MyNamespace:baz", response="WorksAsResponseStruct")]
struct SomeBazRequest {
    param1: String,
    param2: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorksAsResponseStruct {
    value1: u32,
    value2: String,
}
```
With the above code, we have defined two request object, each have been mapped to their own methods:
+ `SomeBarRequest` - "bar_someBar"
+ `SomeBazRequest` - "baz_someBaz"

Now we have defined namespacing for specific kinds of methods in our api! The next step is to create a `RpcRequestWrapper` so we can easily hande parsing all of our requests: 
```rust
// As long as each variant in this enum implements RpcRequest, this derive macro should work
#[derive(RpcRequestWrapper, Debug)]
enum RequestWrapper {
    SomeBaz(SomeBazRequest),
    SomeBar(SomeBarRequest),
}
```
Now when we receive a request through an `RpcListeningThread`, we can coerce it to this wrapper struct and handle all possible requests:
```rust
if let Some(req) = server_thread.recv.recv().await {
    let wrapper = RequestWrapper::try_from_rpc_req(req)?;
    let response = match wrapper {
        RequestWrapper::SomeBaz(r) => // do some work & return a response
        RequestWrapper::SomeBar(r) => // do some work & return a response
    };
    server_thread.sender.send(response).await?;
}
```
## `RpcHandler`
I have also created a trait called `RpcHandler`. It may add *too much* abstraction, so it may be removed in the future, but it compartmentalizes handling requests a little more.
```rust
pub type ProcessRequestResult = Result<serde_json::Value, socket::Error>;
#[allow(async_fn_in_trait)]
pub trait RpcHandler {
    type ReqWrapper: RpcRequestWrapper;
    /// Handler does whatever it does with request and returns either a socket request `result` field, or an error
    async fn process_request(&mut self, req: Self::ReqWrapper) -> MainResult<ProcessRequestResult>;
    async fn handle_rpc_request(&mut self, req: socket::Request) -> MainResult<socket::Response> {
        let req_id = req.id.clone();
        let wrapper = Self::ReqWrapper::try_from_rpc_req(req)?;
        let result = self.process_request(wrapper).await?;
        Ok(socket::Response::from((result, req_id)))
    }
}
```

Since `socket::Response` implements `From<ProcessRequestResult>` it makes managing returning error/successful responses a little easier. But this trait is not required to implement a JSON RPC api.
