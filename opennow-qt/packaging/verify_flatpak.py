import json
import select
import subprocess

from verify_linux_package import verify_capabilities


APP_ID = "io.github.opencloudgaming.OpenNOW"


def flatpak_command(command, *arguments):
    return ["flatpak", "run", "--user", f"--command={command}", APP_ID, *arguments]


def verify_flatpak():
    probe = subprocess.run(
        flatpak_command("opennow-streamer"),
        input='{"id":"package-probe","type":"hello","protocolVersion":6}\n'
              '{"id":"package-shutdown","type":"shutdown"}\n',
        text=True, capture_output=True, check=True, timeout=45,
    )
    messages = [json.loads(line) for line in probe.stdout.splitlines()]
    verify_capabilities(next(message for message in messages if message.get("id") == "package-probe"))
    subprocess.run(flatpak_command("opennow-acceptance-verify", "--help"), check=True, timeout=30)
    core = subprocess.Popen(
        flatpak_command("opennow-core", "--data-dir", "/tmp/opennow-flatpak-check"),
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
    )
    try:
        core.stdin.write('{"type":"request","id":"flatpak-updater","method":"updater.state.get","params":{}}\n')
        core.stdin.flush()
        if not select.select([core.stdout], [], [], 30)[0]:
            raise TimeoutError("The installed Flatpak core did not respond")
        response = json.loads(core.stdout.readline())
        if response.get("id") != "flatpak-updater" or response.get("ok") is not True:
            raise ValueError(f"The installed Flatpak core rejected the updater request: {response}")
        state = response["result"]
        if state.get("updateSource") != "flatpak" or state.get("status") != "unsupported":
            raise ValueError(f"The installed core did not recognize Flatpak: {state}")
        for capability in ("canCheck", "canDownload", "canInstall", "exitRequired"):
            if state.get(capability) is not False:
                raise ValueError(f"Flatpak must disable {capability}")
    finally:
        core.kill()
        core.communicate(timeout=5)
    subprocess.run(
        ["flatpak", "run", "--user", "--env=QT_QPA_PLATFORM=offscreen",
         "--env=OPENNOW_DATA_DIR=/tmp/opennow-flatpak-check",
         APP_ID, "--smoke-test", "--allow-multiple-instances", "--desktop",
         "--route", "home", "--reduced-motion"],
        check=True, timeout=45,
    )


if __name__ == "__main__":
    verify_flatpak()
    print("Installed Flatpak streamer capabilities, external updates, and Qt startup checks passed")
