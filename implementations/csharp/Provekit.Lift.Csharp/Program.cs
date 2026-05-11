using System.CommandLine;
using Provekit.Lift.Csharp;

var rootCommand = new RootCommand("Provekit C# Language Lifter");
var rpcOption = new Option<bool>("--rpc", "Run in RPC mode");
rootCommand.AddOption(rpcOption);
rootCommand.SetHandler((bool rpc) =>
{
    if (rpc)
    {
        RpcServer.Run();
        return;
    }
    Console.Error.WriteLine("usage: provekit-lift-csharp --rpc");
    Environment.Exit(1);
}, rpcOption);
await rootCommand.InvokeAsync(args);
