use crossbeam_channel::TryRecvError;
use seraphic::{
    connection::{Connection, InitializeConnectionMessage},
    error::{Error, ErrorCode, ErrorKind},
    io::IoThreads,
    MsgWrapper, Response, ResponseWrapper, RpcNamespace, RpcRequest, RpcResponse,
};
use seraphic_derive::{RequestWrapper, ResponseWrapper, RpcNamespace, RpcRequest};
use serde::{Deserialize, Serialize};
use std::{io::Write, thread::sleep, time::Duration};

type MyConnection = Connection<InitRequest>;
type MyWrapper = MsgWrapper<ReqWrapper, ResWrapper>;

#[derive(RequestWrapper, Debug)]
enum ReqWrapper {
    Init(InitRequest),
    Foo(FooRequest),
    FooErr(TriggersErrorRequest),
}

#[derive(ResponseWrapper, Debug)]
enum ResWrapper {
    Init(InitResponse),
    Foo(FooResponse),
}

#[derive(RpcNamespace, Clone, Copy, PartialEq, Eq)]
enum NS {
    Init,
    Foo,
}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "NS:init")]
struct InitRequest {}

impl InitializeConnectionMessage for InitRequest {
    const ID: &str = "initialize";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitResponse {}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "NS:foo")]
struct FooRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FooResponse {}

#[derive(RpcRequest, Clone, Deserialize, Serialize, Debug)]
#[rpc_request(namespace = "NS:foo", response = "FooResponse")]
struct TriggersErrorRequest {}

const ADDR: &str = "127.0.0.1:4569";

fn client() -> (MyConnection, IoThreads) {
    MyConnection::connect(ADDR).unwrap()
}

fn server() -> (MyConnection, IoThreads) {
    MyConnection::listen(ADDR).unwrap()
}

fn client_loop(client_conn: MyConnection) {
    tracing::warn!("starting client loop");
    let stdin = std::io::stdin();
    let mut input = String::new();
    let options = ["foo", "err"];
    let mut req_id = 0;
    loop {
        tracing::warn!("type any of the following:\n{options:#?}");

        if let Ok(bytes_read) = stdin.read_line(&mut input) {
            if bytes_read == 0 {
                break;
            }

            let str = input.as_str().trim();
            let mut req = None;

            match str {
                _ if str == options[0] => {
                    tracing::warn!("selected {str} request");
                    req = Some(FooRequest {}.into_request(req_id).unwrap());
                    req_id += 1;
                }
                _ if str == options[1] => {
                    tracing::warn!("selected {str} request");
                    req = Some(TriggersErrorRequest {}.into_request(req_id).unwrap());
                    req_id += 1;
                }
                _ => {
                    tracing::warn!("no request associated with '{str}'");
                }
            }
            std::io::stdout().flush().unwrap();
            input.clear();

            if let Some(r) = req {
                tracing::warn!("client sending: {r:#?}");
                client_conn.sender.send(r.into()).unwrap();
                tracing::warn!("send successful");
            }

            match client_conn.receiver.try_recv() {
                Ok(msg) => {
                    let wrapper = MyWrapper::try_from(msg).expect("failed to get wrapper");
                    match wrapper {
                        MsgWrapper::Shutdown(_) => {
                            tracing::warn!("received shutdown");
                            return;
                        }
                        MsgWrapper::Exit => {
                            tracing::warn!("received exit");
                            return;
                        }
                        MsgWrapper::Req { req, .. } => {
                            tracing::warn!(
                                "client received request {req:#?}, this is unexpected but fine"
                            );
                        }
                        MsgWrapper::Res { res, .. } => {
                            tracing::warn!("client received response {res:#?}");
                        }
                    }
                }
                Err(TryRecvError::Empty) => {
                    tracing::warn!("received empty");
                }
                Err(TryRecvError::Disconnected) => {
                    tracing::warn!("disconnected");
                    return;
                }
            }

            std::io::stdout().flush().unwrap();
        }
    }
    tracing::warn!("Goodbye!");
}

/// Simply sends empty responses associated with received requests
fn server_loop(server_conn: MyConnection) {
    loop {
        match server_conn.receiver.try_recv() {
            Ok(msg) => {
                tracing::warn!("server received: {msg:#?}");
                let wrapper = MyWrapper::try_from(msg).expect("failed to get wrapper");
                match wrapper {
                    MsgWrapper::Req { id, req } => match req {
                        ReqWrapper::Foo(_foo) => {
                            let response = FooResponse {};
                            server_conn
                                .sender
                                .send(response.into_response(id).unwrap().into())
                                .unwrap()
                        }
                        ReqWrapper::Init(_init) => {
                            let response = InitResponse {};
                            server_conn
                                .sender
                                .send(response.into_response(id).unwrap().into())
                                .unwrap()
                        }
                        ReqWrapper::FooErr(_foo_err) => {
                            let error: Error = ErrorKind::other(
                                "received foo err, returning error response",
                                ErrorCode::InternalError,
                            )
                            .into();
                            let response = Response::from_error(id, error);
                            server_conn.sender.send(response.into()).unwrap()
                        }
                    },
                    MsgWrapper::Res { res, .. } => {
                        tracing::warn!(
                            "server received response {res:#?}, this is unexpected but fine"
                        );
                    }
                    MsgWrapper::Shutdown(_) => {
                        tracing::warn!("received shutdown");
                        return;
                    }
                    MsgWrapper::Exit => {
                        tracing::warn!("received exit");
                        return;
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("disconnected");
                return;
            }
        }
    }
}

fn main() {
    // let task = std::thread::spawn(move || {
    //     let (server, threads) = server();
    //     tracing::warn!("server started");
    //     server.initialize(InitResponse {}).unwrap();
    //
    //     server_loop(server);
    //     threads.join().unwrap();
    // });
    // sleep(Duration::from_secs(1));
    //
    // let (client, threads) = client();
    // tracing::warn!("client started");
    // client_loop(client);
    // threads.join().unwrap();
    //
    // assert!(task.is_finished())
}
