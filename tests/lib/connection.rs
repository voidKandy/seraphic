use core::panic;
use std::{
    fs::File,
    io::Write,
    thread::{self, sleep},
    time::Duration,
};

use seraphic::{
    client::Client, connection::InitializeConnectionMessage, error::ErrorCode, server::Server,
};

use crate::{TestConnection, TestInitRequest, TestInitResponse};

#[test]
fn test_client_server_init_works() {
    tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::new())
        .expect("setting default subscriber failed");

    let addr = "127.0.0.1:5567";

    thread::spawn(move || {
        let mut file = File::create("thread.txt").unwrap();
        let init_response = TestInitResponse {};
        let server = Server::from(TestConnection::listen(addr).unwrap());
        writeln!(file, "server create").unwrap();
        let req = server.initialize(init_response).unwrap();
        writeln!(file, "server init").unwrap();
        assert!(TestInitRequest::matches(&req));
        server.threads.join().unwrap();
    });
    let mut file = File::create("output.txt").unwrap();
    sleep(Duration::from_secs(1));
    let client = Client::from(TestConnection::connect(addr).unwrap());
    writeln!(file, "client create").unwrap();

    let res = client.initialize(TestInitRequest {}).unwrap().unwrap();
    writeln!(file, "client init").unwrap();
    client.threads.join().unwrap();
    assert!(TestInitRequest::matches(&res))
}
