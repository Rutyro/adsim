# adsim

A virtual 3-axis XYZ gantry that speaks real ADS over TCP (port 48898), so
TwinCAT clients can drive a simulated machine with no hardware and no runtime
license.

Status: work in progress. ADS symbol services, gantry motion, notifications,
alarms, the live view and the MCP server work. The TwinCAT trial-VM check
against a genuine runtime is still open.

Not affiliated with Beckhoff Automation. ADS/AMS behavior is implemented
clean-room from public documentation.

## What works

- AMS/TCP handshake, ReadState, ReadDeviceInfo
- Symbol upload, handle-by-name, read and write by handle (the official
  `Beckhoff.TwinCAT.Ads` .NET client passes its integration test)
- Simulated gantry: three axes with trapezoid motion, soft limits,
  enable/home/fault/reset
- Value notifications on subscribed symbols, including a global alarm
  (`MAIN.bAlarm`, `MAIN.nAlarmCode`; 1001/1002/1003 = X/Y/Z soft-limit fault)
- `.tmc` import: flat PLC symbols become readable/writable memory; custom
  types are served as raw bytes
- Live browser view via SSE

## Run

```
cargo run -p adsim
```

- ADS: `127.0.0.1:48898`
- View: <http://127.0.0.1:8080/>

Arguments: `adsim [ads_addr] [view_addr]`. Add `--tmc path/to/plc.tmc` to
serve the flat symbols from a TwinCAT module class file alongside the
built-in gantry.

## Drive it with an agent

`adsim-mcp` is a stdio MCP server. It connects to the simulator by default:

```
cargo run -p adsim-mcp
```

Configure it in any MCP client as a stdio server. Tools:

- `list_symbols` — symbol names with ADS type and size
- `read_symbol` — read one symbol as JSON
- `write_symbol` — write one symbol
- `wait_for_symbol` — poll until a value matches or the timeout expires

A typical task is "home all axes, move X to 100 mm, report the alarm code":
the agent writes `MAIN.stAxisX.bEnabled`, `bHomed` and `fSetPos`, then waits
on `fActPos`. A setpoint outside 0–300 raises `MAIN.nAlarmCode`
(1001/1002/1003 = X/Y/Z); fix the setpoint, write `MAIN.bReset`, and the
alarm clears.

### Real target

Simulator only, unless `--real` is given. A real target also needs an
explicit write allowlist:

```
adsim-mcp --ads 192.168.1.10:48898 --real 5.1.2.3.1.1 --write MAIN.stAxisX.fSetPos
```

In real mode every write must name an allowlisted symbol. The real-target
check uses TwinCAT 3 in a private VM (trial runtime, non-production use);
the MCP code path is identical, only the addresses change.

## Test

```
cargo test
dotnet test tests/dotnet
```

`cargo test -p adsim-mcp` drives the whole demo over MCP: home, move X to
100 mm, inject an out-of-range setpoint, observe alarm 1001, reset.

The .NET test needs `target/debug/adsim` to exist first (`cargo build`).

## License

MIT.
