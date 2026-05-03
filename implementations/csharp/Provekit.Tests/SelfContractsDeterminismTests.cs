// SPDX-License-Identifier: Apache-2.0
//
// Determinism gate for Provekit.SelfContracts orchestrator.
//
// Mints the C# self-contracts twice into separate temp dirs and asserts
// the catalog CIDs are byte-identical. This is the same canary the
// orchestrator's Main runs, lifted into xUnit so CI catches regressions.
//
// IMPORTANT: Provekit.IR.Collector is process-global static. This test
// shares the "CollectorSerial" collection with any other test that calls
// BeginCollecting/Finish, so xUnit serializes them — guaranteeing no
// cross-test collector pollution.
//
// No CID value pin yet: first run establishes the CID via the
// orchestrator output; the determinism contract is "two runs in this
// process produce identical CIDs". A future commit can pin the value
// once it stabilizes across editor sessions.

using Provekit.SelfContracts;
using Xunit;

namespace Provekit.Tests;

[Collection("CollectorSerial")]
public class SelfContractsDeterminismTests
{
    [Fact]
    public void MintOneRun_IsByteDeterministicAcrossTwoRuns()
    {
        var tmp1 = Path.Combine(Path.GetTempPath(), $"provekit-csharp-self-{Guid.NewGuid():N}");
        var tmp2 = Path.Combine(Path.GetTempPath(), $"provekit-csharp-self-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tmp1);
        Directory.CreateDirectory(tmp2);
        try
        {
            var (cid1, contractSetCid1, count1, files1) = Program.MintOneRun(tmp1, verbose: false);
            var (cid2, contractSetCid2, count2, files2) = Program.MintOneRun(tmp2, verbose: false);

            Assert.Equal(cid1, cid2);
            Assert.Equal(contractSetCid1, contractSetCid2);
            Assert.Equal(count1, count2);
            Assert.Equal(files1, files2);
            Assert.StartsWith("blake3-512:", cid1);
            Assert.Equal(139, cid1.Length);
            Assert.StartsWith("blake3-512:", contractSetCid1);
            Assert.Equal(139, contractSetCid1.Length);

            // Cross-check: the .proof file actually exists and its filename is the CID.
            var path1 = Path.Combine(tmp1, $"{cid1}.proof");
            var path2 = Path.Combine(tmp2, $"{cid2}.proof");
            Assert.True(File.Exists(path1), $"missing {path1}");
            Assert.True(File.Exists(path2), $"missing {path2}");

            // And the bytes themselves match.
            var bytes1 = File.ReadAllBytes(path1);
            var bytes2 = File.ReadAllBytes(path2);
            Assert.Equal(bytes1, bytes2);
        }
        finally
        {
            try { Directory.Delete(tmp1, recursive: true); } catch { }
            try { Directory.Delete(tmp2, recursive: true); } catch { }
        }
    }
}
