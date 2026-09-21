using System;
using System.Collections.Generic;
using System.IO;

namespace OpenNow.Playnite.Services
{
    internal static class OpenNowPath
    {
        public static IEnumerable<string> GetDefaultCandidatePaths()
        {
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            yield return Path.Combine(localAppData, "OpenNOW", "bin", "OpenNOW.exe");
            yield return Path.Combine(localAppData, "Programs", "OpenNOW", "bin", "OpenNOW.exe");
            yield return Path.Combine(localAppData, "Programs", "OpenNOW", "OpenNOW.exe");
            yield return Path.Combine(localAppData, "OpenNOW", "OpenNOW.exe");

            var programFiles = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles);
            foreach (var candidate in GetMachineCandidatePaths(programFiles))
            {
                yield return candidate;
            }

            var programFilesX86 = Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86);
            foreach (var candidate in GetMachineCandidatePaths(programFilesX86))
            {
                yield return candidate;
            }
        }

        internal static IEnumerable<string> GetMachineCandidatePaths(string directory)
        {
            if (string.IsNullOrWhiteSpace(directory))
            {
                yield break;
            }
            yield return Path.Combine(directory, "OpenNOW", "bin", "OpenNOW.exe");
            var versioned = new List<KeyValuePair<Version, string>>();
            try
            {
                if (Directory.Exists(directory))
                {
                    foreach (var path in Directory.GetDirectories(directory, "OpenNOW *"))
                    {
                        if (Version.TryParse(Path.GetFileName(path).Substring("OpenNOW ".Length), out var version))
                        {
                            versioned.Add(new KeyValuePair<Version, string>(version, path));
                        }
                    }
                }
            }
            catch (IOException)
            {
                versioned.Clear();
            }
            catch (UnauthorizedAccessException)
            {
                versioned.Clear();
            }
            versioned.Sort((left, right) => right.Key.CompareTo(left.Key));
            foreach (var installation in versioned)
            {
                yield return Path.Combine(installation.Value, "bin", "OpenNOW.exe");
            }
            foreach (var name in new[] { "OpenNOW Nightly", "OpenNOW Supporter" })
            {
                yield return Path.Combine(directory, name, "bin", "OpenNOW.exe");
            }
            yield return Path.Combine(directory, "OpenNOW", "OpenNOW.exe");
        }

        public static string ResolveExecutablePath(string configuredPath)
        {
            if (!string.IsNullOrWhiteSpace(configuredPath) && File.Exists(configuredPath))
            {
                return configuredPath;
            }

            foreach (var candidate in GetDefaultCandidatePaths())
            {
                if (File.Exists(candidate))
                {
                    return candidate;
                }
            }

            return null;
        }
    }
}
