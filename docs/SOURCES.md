# Sources

Public references used to build adsim. The implementation is clean-room:
only public documentation, public code and the official client library
were used.

## ADS / AMS protocol

- TwinCAT ADS/AMS Specification, TX1000, version 1.2 (Beckhoff Automation).
  Basis for the AMS/TCP header, AMS header, ADS command IDs, request and
  response layouts, symbol services and the device notification stream.
  Document hub:
  <https://infosys.beckhoff.com/english.php?content=../content/1033/tcinfosys3/11291871243.html&id=>
- TwinCAT 3 ADS .NET V7 reference (Beckhoff Automation).
  Used to verify the behavior of the official .NET client that runs in
  `tests/dotnet`.
  <https://download.beckhoff.com/download/Document/automation/twincat3/TC3_ADS_NET_V7_EN.pdf>

## Public implementations

- SoftBeckhoff, a public C# ADS server. Cross-check for the
  AddNotification response shape, the notification stream layout and the
  AMS state flags used for server-pushed notifications.
  <https://github.com/fbarresi/SoftBeckhoff>
- PLC.tmc sample from the European Spallation Source CHIC_Dev project.
  Cross-check for the TMC XML element structure used by the importer
  (`<Symbol>`, `<Name>`, `<Type>`, `<BitSize>`, `<BitOffs>`).
  <https://bitbucket.org/europeanspallationsource/chic_dev/diff/CHIC_Dev/PLC/PLC.tmc?diff2=3891ff189e71710fa748caf5c67f5dfd958b31fa>

## Tooling

- rmcp, the Rust MCP SDK: <https://docs.rs/rmcp>
- quick-xml: <https://docs.rs/quick-xml>
- Beckhoff.TwinCAT.Ads (official NuGet client), used only by the
  integration tests: <https://www.nuget.org/packages/Beckhoff.TwinCAT.Ads>

## Trademarks

TwinCAT and Beckhoff are trademarks of Beckhoff Automation GmbH.
adsim is not affiliated with or endorsed by Beckhoff Automation.
