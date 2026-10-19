use ams::{
    AmsAddress, AmsHeader, Command, Packet, FLAG_REQUEST, FLAG_RESPONSE, read_device_info_response,
    read_state_response,
};

fn read_state_request() -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&[0, 0]); // AMS/TCP reserved
    frame.extend_from_slice(&32u32.to_le_bytes()); // AMS header length
    frame.extend_from_slice(&[127, 0, 0, 1, 1, 1]); // target NetId
    frame.extend_from_slice(&851u16.to_le_bytes()); // target port
    frame.extend_from_slice(&[192, 168, 1, 10, 1, 1]); // source NetId
    frame.extend_from_slice(&30000u16.to_le_bytes()); // source port
    frame.extend_from_slice(&0x0004u16.to_le_bytes()); // ReadState
    frame.extend_from_slice(&FLAG_REQUEST.to_le_bytes());
    frame.extend_from_slice(&0u32.to_le_bytes()); // data length
    frame.extend_from_slice(&0u32.to_le_bytes()); // error code
    frame.extend_from_slice(&42u32.to_le_bytes()); // invoke id
    frame
}

#[test]
fn decodes_read_state_request() {
    let packet = Packet::decode(&read_state_request()).expect("valid frame");

    assert_eq!(
        packet.header.target,
        AmsAddress::new([127, 0, 0, 1, 1, 1], 851)
    );
    assert_eq!(
        packet.header.source,
        AmsAddress::new([192, 168, 1, 10, 1, 1], 30000)
    );
    assert_eq!(packet.header.command, Command::ReadState);
    assert_eq!(packet.header.flags, FLAG_REQUEST);
    assert_eq!(packet.header.invoke_id, 42);
    assert!(packet.data.is_empty());
}

#[test]
fn encode_round_trips_the_frame() {
    let frame = read_state_request();
    let packet = Packet::decode(&frame).expect("valid frame");

    assert_eq!(packet.encode(), frame);
}

#[test]
fn response_swaps_addresses_and_sets_response_flag() {
    let request = Packet::decode(&read_state_request()).expect("valid frame");
    let response = request.response(read_state_response(0, 5, 0));

    assert_eq!(response.header.target, request.header.source);
    assert_eq!(response.header.source, request.header.target);
    assert_eq!(response.header.command, Command::ReadState);
    assert_eq!(response.header.flags, FLAG_RESPONSE);
    assert_eq!(response.header.invoke_id, request.header.invoke_id);
    assert_eq!(response.header.error_code, 0);
}

#[test]
fn read_state_response_is_result_state_and_device_state() {
    assert_eq!(
        read_state_response(0, 5, 0),
        vec![0, 0, 0, 0, 5, 0, 0, 0]
    );
}

#[test]
fn read_device_info_response_has_version_and_padded_name() {
    let data = read_device_info_response(0, (3, 1, 4024), "adsim");

    assert_eq!(data.len(), 24);
    assert_eq!(&data[0..4], &0u32.to_le_bytes());
    assert_eq!(data[4], 3);
    assert_eq!(data[5], 1);
    assert_eq!(&data[6..8], &4024u16.to_le_bytes());
    assert_eq!(&data[8..13], b"adsim");
    assert!(data[13..].iter().all(|byte| *byte == 0));
}

#[test]
fn rejects_short_frame() {
    assert!(Packet::decode(&[0, 0, 0]).is_err());
}

#[test]
fn rejects_declared_length_that_does_not_match() {
    let mut frame = read_state_request();
    frame[3] = 99;
    assert!(Packet::decode(&frame).is_err());
}

#[test]
fn rejects_unknown_command() {
    let mut frame = read_state_request();
    frame[22..24].copy_from_slice(&0x00ffu16.to_le_bytes());
    assert!(Packet::decode(&frame).is_err());
}

#[test]
fn response_keeps_ams_error_code_zero() {
    let request = Packet {
        header: AmsHeader {
            target: AmsAddress::new([127, 0, 0, 1, 1, 1], 851),
            source: AmsAddress::new([10, 0, 0, 1, 1, 1], 30000),
            command: Command::ReadDeviceInfo,
            flags: FLAG_REQUEST,
            error_code: 0,
            invoke_id: 7,
        },
        data: Vec::new(),
    };

    let response = request.response(read_device_info_response(0, (3, 1, 1), "adsim"));
    assert_eq!(response.header.error_code, 0);
}
