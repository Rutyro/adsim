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
            var (router, routerTask, client, cancel) = await ConnectAsync();
            try
            {
                var result = await client.ReadStateAsync(cancel.Token);
                Assert.Equal(AdsState.Run, result.State.AdsState);

                var value = client.ReadValue("MAIN.stAxisX.fActPos");
                Assert.Equal(0.0, Convert.ToDouble(value));

                var symbol = client.ReadSymbol("MAIN.stAxisX.fActPos");
                Assert.Equal("MAIN.stAxisX.fActPos", symbol.InstancePath);
            }
            finally
            {
                client.Dispose();
                cancel.Dispose();
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

    [Fact]
    public async Task OfficialClientWritesMoveTheAxis()
    {
        using var adsim = StartAdsim();
        try
        {
            await WaitForPort(AdsimAddress, AdsimPort, TimeSpan.FromSeconds(10));
            var (router, routerTask, client, cancel) = await ConnectAsync();
            try
            {
                client.WriteValue("MAIN.stAxisX.bEnabled", true);
                client.WriteValue("MAIN.stAxisX.bHomed", true);
                client.WriteValue("MAIN.stAxisX.fSetPos", 10.0);

                var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(5);
                double position;
                do
                {
                    await Task.Delay(50, cancel.Token);
                    position = Convert.ToDouble(client.ReadValue("MAIN.stAxisX.fActPos"));
                }
                while (Math.Abs(position - 10.0) > 0.01 && DateTime.UtcNow < deadline);

                Assert.InRange(position, 9.99, 10.01);
            }
            finally
            {
                client.Dispose();
                cancel.Dispose();
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

    [Fact]
    public async Task OfficialClientReceivesNotifications()
    {
        using var adsim = StartAdsim();
        try
        {
            await WaitForPort(AdsimAddress, AdsimPort, TimeSpan.FromSeconds(10));
            var (router, routerTask, client, cancel) = await ConnectAsync();
            try
            {
                var notified = new TaskCompletionSource<double>(
                    TaskCreationOptions.RunContinuationsAsynchronously);
                client.AdsNotification += (_, e) =>
                    notified.TrySetResult(BitConverter.ToDouble(e.Data.Span));

                var handle = client.AddDeviceNotification(
                    "MAIN.stAxisX.fSetPos",
                    8,
                    new NotificationSettings(AdsTransMode.OnChange, 50, 0),
                    null);
                client.WriteValue("MAIN.stAxisX.fSetPos", 12.5);

                var value = await notified.Task.WaitAsync(TimeSpan.FromSeconds(5));
                Assert.Equal(12.5, value);
                client.DeleteDeviceNotification(handle);
            }
            finally
            {
                client.Dispose();
                cancel.Dispose();
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

    private static async Task<(
        AmsTcpIpRouter Router,
        Task RouterTask,
        AdsClient Client,
        CancellationTokenSource Cancel)> ConnectAsync()
    {
        var localNetId = new AmsNetId("10.0.0.1.1.1");
        var remoteNetId = new AmsNetId("127.0.0.2.1.1");
        var loggerFactory = NullLoggerFactory.Instance;

        var router = new AmsTcpIpRouter(localNetId, loggerFactory);
        router.AddRoute(new Route("adsim", remoteNetId, new[] { AdsimAddress }));

        var cancel = new CancellationTokenSource(TimeSpan.FromSeconds(20));
        var routerTask = router.StartAsync(cancel.Token);
        while (!router.IsRunning)
        {
            await Task.Delay(50, cancel.Token);
        }

        var client = new AdsClient(loggerFactory);
        client.Connect(new AmsAddress(remoteNetId, 851));
        return (router, routerTask, client, cancel);
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
