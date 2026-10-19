mod common;

use ams::{Command, FLAG_REQUEST, Packet};
use common::{receive, request, send, start_server};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

const SYM_HNDBYNAME: u32 = 0xF003;
const SYM_VALBYHND: u32 = 0xF005;
const SYM_RELEASEHND: u32 = 0xF006;
const SYM_UPLOAD: u32 = 0xF00B;
const SYM_UPLOADINFO2: u32 = 0xF00F;

fn read_request(index_group: u32, index_offset: u32, length: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&index_group.to_le_bytes());
    data.extend_from_slice(&index_offset.to_le_bytes());
    data.extend_from_slice(&length.to_le_bytes());
    data
}

fn write_request(index_group: u32, index_offset: u32, value: &[u8]) -> Vec<u8> {
    let mut data = read_request(index_group, index_offset, value.len() as u32);
    data.extend_from_slice(value);
    data
}

fn handle_request(name: &str, invoke_id: u32) -> Vec<u8> {
    let name = format!("{name}\0");
    let mut data = Vec::new();
    data.extend_from_slice(&SYM_HNDBYNAME.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(name.as_bytes());
    request(Command::ReadWrite, invoke_id, data)
}

fn result(packet: &Packet) -> u32 {
    u32::from_le_bytes(packet.data[0..4].try_into().expect("result"))
}

fn length(packet: &Packet) -> usize {
    u32::from_le_bytes(packet.data[4..8].try_into().expect("length")) as usize
}

fn payload(packet: &Packet) -> &[u8] {
    &packet.data[8..8 + length(packet)]
}

#[tokio::test]
async fn reports_symbol_upload_info2() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(
        &mut stream,
        &request(Command::Read, 1, read_request(SYM_UPLOADINFO2, 0, 64)),
    )
    .await;

    assert_eq!(result(&response), 0);
    assert_eq!(length(&response), 24);
    let info = payload(&response);
    assert_eq!(
        u32::from_le_bytes(info[0..4].try_into().expect("symbol count")),
        18
    );
    assert!(u32::from_le_bytes(info[4..8].try_into().expect("symbol size")) > 0);
    assert_eq!(&info[8..24], &[0u8; 16]);
}

#[tokio::test]
async fn uploads_symbol_entries() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let info_response = send(
        &mut stream,
        &request(Command::Read, 1, read_request(SYM_UPLOADINFO2, 0, 64)),
    )
    .await;
    let info = payload(&info_response);
    let upload_size = u32::from_le_bytes(info[4..8].try_into().expect("symbol size"));

    let response = send(
        &mut stream,
        &request(Command::Read, 2, read_request(SYM_UPLOAD, 0, upload_size)),
    )
    .await;

    assert_eq!(result(&response), 0);
    assert_eq!(length(&response), upload_size as usize);
    let blob = payload(&response);
    assert!(
        blob.windows(b"MAIN.stAxisX.fActPos".len())
            .any(|window| window == b"MAIN.stAxisX.fActPos")
    );
    assert!(blob.windows(5).any(|window| window == b"LREAL"));
}

#[tokio::test]
async fn reads_and_writes_a_symbol_through_a_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(&mut stream, &handle_request("MAIN.stAxisX.fSetPos", 10)).await;
    assert_eq!(result(&response), 0);
    assert_eq!(length(&response), 4);
    let handle = u32::from_le_bytes(payload(&response)[0..4].try_into().expect("handle"));
    assert_ne!(handle, 0);

    let response = send(
        &mut stream,
        &request(Command::Read, 11, read_request(SYM_VALBYHND, handle, 8)),
    )
    .await;
    assert_eq!(result(&response), 0);
    assert_eq!(payload(&response), &[0u8; 8]);

    let response = send(
        &mut stream,
        &request(
            Command::Write,
            12,
            write_request(SYM_VALBYHND, handle, &1.5f64.to_le_bytes()),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);

    let response = send(
        &mut stream,
        &request(Command::Read, 13, read_request(SYM_VALBYHND, handle, 8)),
    )
    .await;
    assert_eq!(payload(&response), &1.5f64.to_le_bytes());
}

#[tokio::test]
async fn releases_a_symbol_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(&mut stream, &handle_request("MAIN.stAxisX.fActPos", 20)).await;
    let handle = u32::from_le_bytes(payload(&response)[0..4].try_into().expect("handle"));

    let response = send(
        &mut stream,
        &request(
            Command::Write,
            21,
            write_request(SYM_RELEASEHND, 0, &handle.to_le_bytes()),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);

    let response = send(
        &mut stream,
        &request(Command::Read, 22, read_request(SYM_VALBYHND, handle, 8)),
    )
    .await;
    assert_eq!(result(&response), 0x0701);
    assert_eq!(length(&response), 0);
}

#[tokio::test]
async fn reads_symbol_info_by_name() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let name = b"MAIN.stAxisX.fActPos\0";
    let mut data = Vec::new();
    data.extend_from_slice(&0xF009u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&4096u32.to_le_bytes());
    data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    data.extend_from_slice(name);

    let response = send(&mut stream, &request(Command::ReadWrite, 25, data)).await;

    assert_eq!(result(&response), 0);
    let entry = payload(&response);
    let entry_len = u32::from_le_bytes(entry[0..4].try_into().expect("entry length"));
    assert_eq!(entry_len as usize, entry.len());
    assert_eq!(&entry[4..8], &0x4020u32.to_le_bytes());
    assert_eq!(&entry[30..50], b"MAIN.stAxisX.fActPos");
    assert_eq!(entry[50], 0);
}

#[tokio::test]
async fn add_notification_returns_result_and_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let mut data = Vec::new();
    data.extend_from_slice(&0xF008u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());

    let response = send(&mut stream, &request(Command::AddNotification, 26, data)).await;

    assert_eq!(result(&response), 0);
    assert_eq!(response.data.len(), 8);
    assert_ne!(
        u32::from_le_bytes(response.data[4..8].try_into().expect("handle")),
        0
    );
}

#[tokio::test]
async fn deletes_a_notification_handle() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let handle = get_handle(&mut stream, "MAIN.stAxisX.fActPos").await;

    let mut data = Vec::new();
    data.extend_from_slice(&SYM_VALBYHND.to_le_bytes());
    data.extend_from_slice(&handle.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    let response = send(&mut stream, &request(Command::AddNotification, 27, data)).await;
    assert_eq!(result(&response), 0);
    let notification_handle =
        u32::from_le_bytes(response.data[4..8].try_into().expect("notification handle"));

    let response = send(
        &mut stream,
        &request(
            Command::DeleteNotification,
            28,
            notification_handle.to_le_bytes().to_vec(),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);

    let response = send(
        &mut stream,
        &request(
            Command::DeleteNotification,
            29,
            notification_handle.to_le_bytes().to_vec(),
        ),
    )
    .await;
    assert_eq!(result(&response), 0x0701);
}

async fn get_handle(stream: &mut TcpStream, name: &str) -> u32 {
    let response = send(stream, &handle_request(name, 100)).await;
    assert_eq!(result(&response), 0);
    u32::from_le_bytes(payload(&response)[0..4].try_into().expect("handle"))
}

async fn write_value(stream: &mut TcpStream, handle: u32, value: &[u8]) {
    let response = send(
        stream,
        &request(
            Command::Write,
            101,
            write_request(SYM_VALBYHND, handle, value),
        ),
    )
    .await;
    assert_eq!(result(&response), 0);
}

async fn read_value(stream: &mut TcpStream, handle: u32) -> Vec<u8> {
    let response = send(
        stream,
        &request(Command::Read, 102, read_request(SYM_VALBYHND, handle, 8)),
    )
    .await;
    assert_eq!(result(&response), 0);
    payload(&response).to_vec()
}

#[tokio::test]
async fn notifies_a_subscribed_symbol_when_it_changes() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let handle = get_handle(&mut stream, "MAIN.stAxisX.fSetPos").await;

    let mut data = Vec::new();
    data.extend_from_slice(&SYM_VALBYHND.to_le_bytes());
    data.extend_from_slice(&handle.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    let response = send(&mut stream, &request(Command::AddNotification, 50, data)).await;
    assert_eq!(result(&response), 0);
    assert_eq!(response.data.len(), 8);
    let notification_handle =
        u32::from_le_bytes(response.data[4..8].try_into().expect("notification handle"));

    stream
        .write_all(&request(
            Command::Write,
            101,
            write_request(SYM_VALBYHND, handle, &1.25f64.to_le_bytes()),
        ))
        .await
        .expect("write request");

    let notification = loop {
        let packet = receive(&mut stream).await;
        if packet.header.command == Command::Notification {
            break packet;
        }
        assert_eq!(result(&packet), 0);
    };
    assert_eq!(notification.header.flags, FLAG_REQUEST);

    let data = &notification.data;
    assert_eq!(
        u32::from_le_bytes(data[0..4].try_into().expect("payload length")),
        28
    );
    assert_eq!(
        u32::from_le_bytes(data[4..8].try_into().expect("stamp count")),
        1
    );
    assert_ne!(
        u64::from_le_bytes(data[8..16].try_into().expect("timestamp")),
        0
    );
    assert_eq!(
        u32::from_le_bytes(data[16..20].try_into().expect("sample count")),
        1
    );
    assert_eq!(
        u32::from_le_bytes(data[20..24].try_into().expect("notification handle")),
        notification_handle
    );
    assert_eq!(
        u32::from_le_bytes(data[24..28].try_into().expect("sample size")),
        8
    );
    assert_eq!(&data[28..36], &1.25f64.to_le_bytes());
}

#[tokio::test]
async fn an_enabled_axis_moves_to_its_setpoint() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let enabled = get_handle(&mut stream, "MAIN.stAxisX.bEnabled").await;
    let homed = get_handle(&mut stream, "MAIN.stAxisX.bHomed").await;
    let setpoint = get_handle(&mut stream, "MAIN.stAxisX.fSetPos").await;
    let position = get_handle(&mut stream, "MAIN.stAxisX.fActPos").await;
    let error = get_handle(&mut stream, "MAIN.stAxisX.nError").await;

    write_value(&mut stream, enabled, &[1]).await;
    write_value(&mut stream, homed, &[1]).await;
    write_value(&mut stream, setpoint, &10.0f64.to_le_bytes()).await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let value = read_value(&mut stream, position).await;
        let actual = f64::from_le_bytes(value[..8].try_into().expect("position"));
        if (actual - 10.0).abs() < 0.01 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "axis did not reach target: {actual}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let value = read_value(&mut stream, error).await;
    assert_eq!(i32::from_le_bytes(value[..4].try_into().expect("error")), 0);
}

#[tokio::test]
async fn reads_a_symbol_by_index_group_and_offset() {
    let addr = start_server().await;
    let mut stream = TcpStream::connect(addr).await.expect("connect");

    let response = send(
        &mut stream,
        &request(Command::Read, 30, read_request(0x4020, 0, 8)),
    )
    .await;

    assert_eq!(result(&response), 0);
    assert_eq!(payload(&response), &[0u8; 8]);
}
