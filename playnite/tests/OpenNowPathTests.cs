using System;
using System.IO;
using System.Linq;
using OpenNow.Playnite.Services;

public static class OpenNowPathTests
{
    public static void Run()
    {
        var candidates = OpenNowPath.GetDefaultCandidatePaths().ToList();
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        Require(candidates[0] == Path.Combine(local, "OpenNOW", "bin", "OpenNOW.exe"),
            "The current per-user Qt installation must be tried first");
        Require(candidates.Contains(Path.Combine(local, "Programs", "OpenNOW", "bin", "OpenNOW.exe")),
            "Missing the per-user Programs Qt layout");
        foreach (var folder in new[] { Environment.SpecialFolder.ProgramFiles, Environment.SpecialFolder.ProgramFilesX86 })
        {
            var directory = Environment.GetFolderPath(folder);
            if (string.IsNullOrWhiteSpace(directory))
            {
                continue;
            }
            foreach (var channel in new[] { "OpenNOW", "OpenNOW Nightly", "OpenNOW Supporter" })
            {
                Require(candidates.Contains(Path.Combine(directory, channel, "bin", "OpenNOW.exe")),
                    "Missing Qt MSI layout: " + directory + "/" + channel);
            }
            Require(candidates.Contains(Path.Combine(directory, "OpenNOW", "OpenNOW.exe")),
                "Existing executable discovery candidates must be preserved");
        }
        foreach (var directory in new[] { local, Path.Combine(local, "Programs") })
        {
            Require(candidates.Contains(Path.Combine(directory, "OpenNOW", "OpenNOW.exe")),
                "Existing per-user discovery candidates must be preserved");
        }

        var root = Path.Combine(Path.GetTempPath(), "opennow-path-tests-" + Guid.NewGuid().ToString("N"));
        try
        {
            Directory.CreateDirectory(Path.Combine(root, "custom install", "bin"));
            var configured = Path.Combine(root, "custom install", "bin", "OpenNOW.exe");
            File.WriteAllText(configured, "fixture");
            Require(OpenNowPath.ResolveExecutablePath(configured) == configured,
                "An existing explicitly configured executable must take precedence");
            var expected = candidates.FirstOrDefault(File.Exists);
            foreach (var invalid in new[] { null, "", " ", root, Path.Combine(root, "missing.exe") })
            {
                Require(OpenNowPath.ResolveExecutablePath(invalid) == expected,
                    "Invalid configured paths must fall back to the default candidates");
            }
            foreach (var name in new[] { "OpenNOW 1.0.2", "OpenNOW 1.0.10", "OpenNOW Nightly", "OpenNOW Supporter", "OpenNOW unrelated" })
            {
                var bin = Path.Combine(root, name, "bin");
                Directory.CreateDirectory(bin);
                File.WriteAllText(Path.Combine(bin, "OpenNOW.exe"), "fixture");
            }
            var machine = OpenNowPath.GetMachineCandidatePaths(root).ToList();
            Require(machine.Where(File.Exists).First() == Path.Combine(root, "OpenNOW 1.0.10", "bin", "OpenNOW.exe"),
                "Versioned stable MSI installations must be detected in numeric version order");
            Require(machine.Contains(Path.Combine(root, "OpenNOW Nightly", "bin", "OpenNOW.exe")),
                "Nightly MSI installation was not detected");
            Require(machine.Contains(Path.Combine(root, "OpenNOW Supporter", "bin", "OpenNOW.exe")),
                "Supporter MSI installation was not detected");
            Require(!machine.Contains(Path.Combine(root, "OpenNOW unrelated", "bin", "OpenNOW.exe")),
                "An unrelated directory was accepted as a versioned installation");
            Require(!OpenNowPath.GetMachineCandidatePaths("").Any(),
                "Missing system directories must not produce relative candidates");
            Require(OpenNowPath.GetMachineCandidatePaths(Path.Combine(root, "absent")).Any(),
                "Missing installation directories must not throw");
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, true);
            }
        }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
