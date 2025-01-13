use super::*;
use seraphic::RequestWrapper;
use tracing::Level;

#[test]
fn message_serde() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    let rq = MyRequest::from(TestRequest {});
    let expected_req = rq.clone();

    let message = rq.into_message::<MyResponse>(0);
    let expected_message = message.clone();

    let packet = MessagePacket::from(&message);
    let expected_packet = packet.clone();

    let serialized = serde_json::to_vec(&packet).unwrap();

    let packet: MessagePacket = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(packet, expected_packet);

    let message: Message = packet.try_into_inner().unwrap();
    assert_eq!(message, expected_message);

    if let Message::Req { req, .. } = message {
        assert_eq!(expected_req, req)
    } else {
        panic!()
    }

    let rs = MyResponse::from(TestResponse {});
    let expected_req = rs.clone();

    let message = rs.into_message::<MyRequest>(0);
    let expected_message = message.clone();

    let packet = MessagePacket::from(&message);
    let expected_packet = packet.clone();

    let serialized = serde_json::to_vec(&packet).unwrap();

    let packet: MessagePacket = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(packet, expected_packet);

    let message: Message = packet.try_into_inner().unwrap();
    assert_eq!(message, expected_message);

    if let Message::Res { res, .. } = message {
        assert_eq!(expected_req, res)
    } else {
        panic!()
    }
}
