using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using Microsoft.Extensions.Logging.Abstractions;
using TwinCAT.Ads;
using TwinCAT.Ads.Configuration;
using TwinCAT.Ads.TcpRouter;

namespace Adsim.IntegrationTests;

public class ReadStateTests
{
    private static readonly IPAddress AdsimAddress = IPAddress.Parse("127.0.0.2");
    private const int AdsimPort = 48898;

    [Fact]
    public async Task OfficialClientReadStateSucceeds()
    {
        using var adsim = StartAdsim();
        try
        {
            await WaitForPort(AdsimAddress, AdsimPort, TimeSpan.FromSeconds(10));

            var localNetId = new AmsNetId("10.0.0.1.1.1");
            var remoteNetId = new AmsNetId("127.0.0.2.1.1");
            var loggerFactory = NullLoggerFactory.Instance;

            var router = new AmsTcpIpRouter(localNetId, loggerFactory);
            router.AddRoute(new Route("adsim", remoteNetId, new[] { AdsimAddress }));

            using var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(20));
            var routerTask = router.StartAsync(cancel.Token);
            try
            {
                while (!router.IsRunning)
                {
                    await Task.Delay(50, cancel.Token);
                }

                using var client = new AdsClient(loggerFactory);
                client.Connect(new AmsAddress(remoteNetId, 851));

                var result = await client.ReadStateAsync(cancel.Token);
                Assert.Equal(AdsState.Run, result.State.AdsState);

                var value = client.ReadValue("MAIN.stAxisX.fActPos");
                Assert.Equal(0.0, Convert.ToDouble(value));

                var symbol = client.ReadSymbol("MAIN.stAxisX.fActPos");
                Assert.Equal("MAIN.stAxisX.fActPos", symbol.InstancePath);
            }
            finally
            {
                router.Stop();
                await routerTask.WaitAsync(TimeSpan.FromSeconds(5));
            }
        }
        finally
        {
            if (!adsim.HasExited)
            {
                adsim.Kill(entireProcessTree: true);
                adsim.WaitForExit(2000);
            }
        }
    }

    private static Process StartAdsim()
    {
        var binary = FindAdsimBinary();
        var startInfo = new ProcessStartInfo(binary, $"{AdsimAddress}:{AdsimPort}")
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardError = true,
        };
        startInfo.Environment["ADSIM_TRACE"] = "1";

        var process = Process.Start(startInfo)
            ?? throw new InvalidOperationException("adsim did not start");
        process.ErrorDataReceived += (_, line) =>
        {
            if (line.Data is not null)
            {
                Console.WriteLine(line.Data);
            }
        };
        process.BeginErrorReadLine();
        return process;
    }

    private static string FindAdsimBinary()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null
               && !File.Exists(Path.Combine(directory.FullName, "Cargo.toml")))
        {
            directory = directory.Parent;
        }

        if (directory is null)
        {
            throw new InvalidOperationException("repository root (Cargo.toml) not found");
        }

        var name = OperatingSystem.IsWindows() ? "adsim.exe" : "adsim";
        var path = Path.Combine(directory.FullName, "target", "debug", name);
        if (!File.Exists(path))
        {
            throw new InvalidOperationException($"build adsim first: {path}");
        }
        return path;
    }

    private static async Task WaitForPort(IPAddress address, int port, TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            try
            {
                using var probe = new TcpClient();
                await probe.ConnectAsync(address, port);
                return;
            }
            catch (SocketException)
            {
                await Task.Delay(100);
            }
        }

        throw new TimeoutException($"adsim did not listen on {address}:{port}");
    }
}
