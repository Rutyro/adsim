use adsim_client::AdsClient;
use serde_json::json;
use tokio::net::TcpListener;

async fn connect() -> AdsClient {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(adsim_server::serve(listener));
    AdsClient::connect(&addr.to_string(), [127, 0, 0, 1, 1, 1])
        .await
        .expect("connect")
}

#[tokio::test]
async fn lists_symbols_from_the_upload() {
    let client = connect().await;
    let symbols = client.symbols();

    let alarm = symbols
        .iter()
        .find(|symbol| symbol.name == "MAIN.nAlarmCode")
        .expect("alarm symbol");
    assert_eq!(alarm.type_name, "INT32");
    assert_eq!(alarm.size, 4);
    assert_eq!(symbols.len(), 18);
}

#[tokio::test]
async fn reads_a_symbol_as_json() {
    let mut client = connect().await;

    let value = client
        .read_symbol("MAIN.stAxisX.fActPos")
        .await
        .expect("read");
    assert_eq!(value, json!(0.0));

    let homed = client
        .read_symbol("MAIN.stAxisX.bHomed")
        .await
        .expect("read");
    assert_eq!(homed, json!(false));
}

#[tokio::test]
async fn writes_symbols_and_waits_for_the_axis() {
    let mut client = connect().await;

    client
        .write_symbol("MAIN.stAxisX.bEnabled", &json!(true))
        .await
        .expect("enable");
    client
        .write_symbol("MAIN.stAxisX.bHomed", &json!(true))
        .await
        .expect("home");
    client
        .write_symbol("MAIN.stAxisX.fSetPos", &json!(10.0))
        .await
        .expect("setpoint");

    let (value, reached) = client
        .wait_for_symbol("MAIN.stAxisX.fActPos", &json!(10.0), 5000)
        .await
        .expect("wait");
    assert!(reached);
    assert_eq!(value, json!(10.0));
}

#[tokio::test]
async fn reports_an_unknown_symbol() {
    let mut client = connect().await;

    let error = client
        .read_symbol("MAIN.doesNotExist")
        .await
        .expect_err("unknown symbol");
    assert!(error.to_string().contains("unknown symbol"));
}

#[tokio::test]
async fn a_wait_times_out_with_the_last_value() {
    let mut client = connect().await;

    let (value, reached) = client
        .wait_for_symbol("MAIN.stAxisX.fActPos", &json!(99.0), 150)
        .await
        .expect("wait");
    assert!(!reached);
    assert_eq!(value, json!(0.0));
}
