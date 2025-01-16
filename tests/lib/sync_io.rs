use seraphic::packet::{PacketRead, TcpPacket};
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct TestData {
    id: u32,
    message: String,
}

fn handle_client(mut stream: TcpStream) {
    let mut reader = BufReader::new(&mut stream);
    let received: PacketRead<TestData> = TcpPacket::<TestData>::read(&mut reader).unwrap();
    assert_eq!(
        received,
        PacketRead::Message(TestData {
            id: 1,
            message: "Hello".into()
        })
    );
}

#[test]
fn test_tcp_packet_read_write() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => handle_client(stream),
                Err(e) => panic!("Connection failed: {e}"),
            }
        }
    });

    thread::sleep(std::time::Duration::from_millis(100));

    let mut stream = TcpStream::connect("127.0.0.1:7878").unwrap();
    let test_data = TestData {
        id: 1,
        message: "Hello".into(),
    };
    TcpPacket::write(&mut stream, &test_data).unwrap();
}
