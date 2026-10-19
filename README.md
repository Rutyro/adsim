# adsim

A virtual 3-axis XYZ gantry that speaks real ADS over TCP (port 48898), so
TwinCAT clients can drive a simulated machine with no hardware and no runtime
license.

Status: work in progress. ADS symbol services, gantry motion, notifications
and the live view work. The MCP agent is next.

Not affiliated with Beckhoff Automation. ADS/AMS behavior is implemented
clean-room from public documentation.

## What works

- AMS/TCP handshake, ReadState, ReadDeviceInfo
- Symbol upload, handle-by-name, read and write by handle (the official
  `Beckhoff.TwinCAT.Ads` .NET client passes its integration test)
- Simulated gantry: three axes with trapezoid motion, soft limits,
  enable/home/fault/reset
- Value notifications on subscribed symbols
- Live browser view via SSE

## Run

```
cargo run -p adsim
```

- ADS: `127.0.0.1:48898`
- View: <http://127.0.0.1:8080/>

Arguments: `adsim [ads_addr] [view_addr]`.

## Test

```
cargo test
dotnet test tests/dotnet
```

The .NET test needs `target/debug/adsim` to exist first (`cargo build`).

## License

MIT.
