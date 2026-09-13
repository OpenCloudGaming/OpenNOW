import argparse
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[3]


def vendor_sources(root):
    destination = root / "build/flatpak/cargo"
    destination.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [
            "cargo", "vendor", "--locked", "--versioned-dirs",
            "--manifest-path", str(root / "native/opennow-core/Cargo.toml"),
            "--sync", str(root / "native/opennow-streamer/Cargo.toml"),
            "vendor",
        ],
        cwd=destination, check=True, text=True, stdout=subprocess.PIPE,
    )
    config = result.stdout.replace(
        'directory = "vendor"', 'directory = "/run/build/opennow/cargo/vendor"'
    )
    if config == result.stdout:
        raise ValueError("Cargo did not emit the expected vendor directory configuration")
    (destination / "config.toml").write_text(config)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Vendor both locked Rust workspaces for offline Flatpak builds")
    parser.parse_args()
    vendor_sources(ROOT)
