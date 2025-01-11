use crate::{TestInitRequest, TestInitResponse};
use crossbeam_channel::TryRecvError;
use seraphic::{
    client::ClientConnection,
    connection::InitializeConnectionMessage,
    server::{Server, ServerConnection, ServerConnectionHandler},
    Message,
};
use std::{
    net::Shutdown,
    sync::{atomic::AtomicBool, Arc},
    thread::{self, sleep, JoinHandle},
    time::Duration,
};
use tracing::Level;

const ADDR: &str = "127.0.0.1:5567";

#[test]
fn test_client_server_init_works() {
    struct ServerConnHandler;
    impl ServerConnectionHandler<TestInitRequest> for ServerConnHandler {
        fn handler(
            conn: &mut ServerConnection<TestInitRequest>,
        ) -> seraphic::server::ServerHandlerResult {
            let init_response = TestInitResponse {};
            let req = conn.initialize(init_response).unwrap();
            assert!(TestInitRequest::matches(&req));
            tracing::warn!("server received init, ready to shutdown");
            let next = conn.conn.receiver.recv().unwrap();
            assert!(matches!(next, seraphic::Message::Shutdown(false)));
            assert!(
                conn.conn.handle_shutdown(&next).unwrap(),
                "expected to receive a shutdown"
            );

            Ok(())
        }
    }

    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let server_thread: JoinHandle<
        Result<Server<TestInitRequest, ServerConnHandler>, &'static str>,
    > = thread::spawn(move || {
        let server_span = tracing::span!(Level::INFO, "server_thread", ADDR = ADDR);
        let _enter = server_span.enter();

        let mut server = Server::<TestInitRequest, ServerConnHandler>::listen(ADDR).unwrap();
        loop {
            match server.next() {
                Some(res) => {
                    let (conn, shutdown) = res.unwrap();
                    tracing::debug!("server connected!");
                    server.spawn_connection_thread(conn, shutdown);
                    break;
                }
                None => {}
            }
        }
        Ok(server)
    });

    sleep(Duration::from_secs(1));

    let client_thread = thread::spawn(move || {
        let client_span = tracing::span!(Level::INFO, "client_thread", ADDR = ADDR);
        let _enter = client_span.enter();

        let client = ClientConnection::connect(ADDR).unwrap();

        let res = client.initialize(TestInitRequest {}).unwrap().unwrap();

        assert!(TestInitRequest::matches(&res));
        tracing::warn!("client sending shutdown");
        client
            .conn
            .sender
            .send(seraphic::Message::Shutdown(false))
            .unwrap();

        let next = client.conn.receiver.recv().unwrap();
        assert!(matches!(next, seraphic::Message::Shutdown(true)));
        assert!(
            client.conn.handle_shutdown(&next).unwrap(),
            "expected to receive a shutdown"
        );

        client.conn.io_threads.join().unwrap();
    });

    assert!(client_thread.join().is_ok());
    let mut server = server_thread.join().unwrap().unwrap();

    tracing::debug!("shutting down server");

    server.shutdown_and_join_all_connections().unwrap();
    server.shutdown();
}

#[test]
fn test_concurrent_connections_to_server() {
    struct ServerConnHandler;
    impl ServerConnectionHandler<TestInitRequest> for ServerConnHandler {
        fn handler(
            conn: &mut ServerConnection<TestInitRequest>,
        ) -> seraphic::server::ServerHandlerResult {
            let init_rq = conn.initialize(TestInitResponse {}).unwrap();
            tracing::debug!("server received init request from client: {init_rq:#?}");

            loop {
                match conn.conn.receiver.try_recv() {
                    Ok(msg) => match msg {
                        Message::Shutdown(_) => {
                            assert!(conn.conn.handle_shutdown(&msg).unwrap());
                            break;
                        }
                        _ => {
                            tracing::warn!("did not expect to receive: {msg:#?}");
                        }
                    },
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break,
                }
            }

            // conn.conn.io_threads.join().unwrap();
            Ok(())
        }
    }

    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    let amt_clients = 1;
    let mut server = Server::<TestInitRequest, ServerConnHandler>::listen(ADDR).unwrap();
    let server_thread: JoinHandle<
        Result<Server<TestInitRequest, ServerConnHandler>, &'static str>,
    > = thread::spawn(move || {
        while let Some(conn_res) = server.next() {
            let (conn, shutdown) = conn_res.unwrap();
            let addr = server.spawn_connection_thread(conn, shutdown);
            tracing::warn!("added handler for client at {addr:#?}");
            if server.connected_clients().len() == amt_clients {
                break;
            }
        }

        return Ok(server);
    });

    let mut client_threads = vec![];
    for _ in 1..=amt_clients {
        let client_thread = thread::spawn(move || {
            let client = ClientConnection::connect(ADDR).unwrap();
            let client_span = tracing::span!(
                Level::INFO,
                "client_thread",
                ADDR = client.local_addr.to_string()
            );
            let _enter = client_span.enter();
            tracing::debug!("client connected to server");

            // Send initialization request to the server
            let r = client.initialize(TestInitRequest {}).unwrap().unwrap();
            assert!(TestInitRequest::matches(&r));
            tracing::debug!("client got initialization response from server: {r:#?}");

            // Wait a few seconds before shutting down
            thread::sleep(Duration::from_secs(2));

            // Send shutdown message to server
            let _ = client
                .conn
                .sender
                .send(seraphic::Message::Shutdown(false))
                .unwrap();

            loop {
                match client.conn.receiver.try_recv() {
                    Ok(msg) => match msg {
                        Message::Shutdown(_) => {
                            client.conn.handle_shutdown(&msg).unwrap();
                            // client.shutdown();
                            break;
                        }
                        _ => {
                            tracing::warn!("did not expect to receive: {msg:#?}");
                        }
                    },
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break,
                }
            }

            client.conn.io_threads.join().unwrap();
        });

        client_threads.push(client_thread);
    }

    thread::sleep(Duration::from_millis(500));

    // Wait for all client threads to finish
    tracing::warn!("joining all client threads");
    for client_thread in client_threads {
        client_thread.join().unwrap();
    }
    tracing::warn!("all client threads joined");

    let mut server = server_thread.join().unwrap().unwrap();
    tracing::warn!("got server from thread");

    server.shutdown_and_join_all_connections().unwrap();
    server.shutdown();
}
