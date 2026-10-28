import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import time


@dataclass
class Lane:
    name: str
    commands: list[list[str]]
    environment: dict[str, str]


def budgets(parallel, count):
    if parallel < count:
        raise ValueError(f"At least {count} jobs are required for {count} independent lanes")
    jobs, extra = divmod(parallel, count)
    return [jobs + (index < extra) for index in range(count)]


def make_lanes(phase, parallel, environment):
    core = ["--locked", "--manifest-path", "native/opennow-core/Cargo.toml"]
    streamer = ["--locked", "--manifest-path", "native/opennow-streamer/Cargo.toml"]
    if phase == "lint":
        commands = [
            ("core", [["cargo", "clippy", *core, "--all-targets", "--", "-D", "warnings"]]),
            ("streamer", [["cargo", "clippy", *streamer, "--workspace", "--all-targets", "--", "-D", "warnings"]]),
        ]
    elif phase == "build":
        configure = [
            "cmake", "-S", "opennow-qt", "-B", "build/opennow-qt-checks", "-G", "Ninja",
            "-DCMAKE_BUILD_TYPE=Debug", "-DCMAKE_C_COMPILER_LAUNCHER=ccache",
            "-DCMAKE_CXX_COMPILER_LAUNCHER=ccache",
            f"-DCMAKE_PREFIX_PATH={environment['RUNNER_TEMP']}/SDL-install",
        ]
        if environment.get("RUNNER_OS") == "Windows":
            configure.extend(["-DCMAKE_C_COMPILER=cl", "-DCMAKE_CXX_COMPILER=cl"])
        elif environment.get("RUNNER_OS") == "macOS":
            configure.extend([
                f"-DCMAKE_OSX_ARCHITECTURES={environment['TARGET_ARCH']}",
                f"-DCMAKE_OSX_DEPLOYMENT_TARGET={environment['MACOSX_DEPLOYMENT_TARGET']}",
            ])
        commands = [
            ("core", [["cargo", "test", *core, "--no-run", "--timings"]]),
            ("streamer", [["cargo", "test", *streamer, "--workspace", "--no-run", "--timings"]]),
            ("qt", [configure, ["cmake", "--build", "build/opennow-qt-checks", "--target", "opennow-ci-unit-tests"]]),
        ]
    elif phase == "test":
        commands = [
            ("core", [["cargo", "test", *core], [sys.executable, "opennow-qt/tests/run_update_helper_integration.py"]]),
            ("streamer", [["cargo", "test", *streamer, "--workspace"]]),
            ("qt", [["ctest", "--test-dir", "build/opennow-qt-checks", "--output-on-failure", "--no-tests=error",
                "-L", "ci-unit", "--output-junit", "qt-unit-tests.xml"]]),
        ]
    else:
        raise ValueError(f"Unknown phase: {phase}")
    lanes = []
    slots = budgets(parallel, len(commands))
    if phase == "test" and (parallel >= 8 or parallel == 4):
        slots = [parallel // 4, parallel // 4, parallel - 2 * (parallel // 4)]
    for (name, tasks), jobs in zip(commands, slots):
        env = {**environment, "CARGO_BUILD_JOBS": str(jobs), "RUST_TEST_THREADS": str(jobs)}
        if name == "qt":
            tasks[-1].extend(["--parallel", str(jobs)])
        if name == "core" and phase == "test":
            env["OPENNOW_UPDATE_TEST_TARGET_DIR"] = str(Path(environment["OPENNOW_CHECKOUT"]) / "native/opennow-core/target")
        lanes.append(Lane(name, tasks, env))
    return lanes


def run_lanes(lanes, log_directory):
    log_directory.mkdir(parents=True, exist_ok=True)
    active = {}
    results = []
    try:
        for lane in lanes:
            log = (log_directory / f"{lane.name}.log").open("w", encoding="utf-8")
            active[lane.name] = {"lane": lane, "log": log, "index": 0, "process": None, "start": time.monotonic()}
        while active:
            for name, state in list(active.items()):
                process = state["process"]
                code = process.poll() if process is not None else 0
                if code is None:
                    continue
                lane = state["lane"]
                if code == 0 and state["index"] < len(lane.commands):
                    command = lane.commands[state["index"]]
                    state["index"] += 1
                    state["log"].write(f"$ {shlex.join(command)}\n")
                    state["log"].flush()
                    print(f"Starting {name}: {shlex.join(command)}", flush=True)
                    try:
                        state["process"] = subprocess.Popen(
                            command, env=lane.environment, stdout=state["log"], stderr=subprocess.STDOUT,
                            start_new_session=os.name != "nt",
                        )
                        continue
                    except OSError as error:
                        state["log"].write(f"{error}\n")
                        code = 1
                seconds = round(time.monotonic() - state["start"], 2)
                state["log"].close()
                print(f"::group::{name}: exit {code}, {seconds}s", flush=True)
                print((log_directory / f"{name}.log").read_text(encoding="utf-8", errors="replace"), flush=True)
                print("::endgroup::", flush=True)
                if code:
                    print(f"::error::{name} failed with exit code {code}", flush=True)
                    if name == "qt":
                        for filename in ("LastTestsFailed.log", "LastTest.log"):
                            path = Path("build/opennow-qt-checks/Testing/Temporary") / filename
                            if path.is_file():
                                print(path.read_text(encoding="utf-8", errors="replace"), flush=True)
                results.append({"lane": name, "exit_code": code, "seconds": seconds})
                del active[name]
            if active:
                time.sleep(0.1)
    finally:
        for state in active.values():
            process = state["process"]
            if process is not None and process.poll() is None:
                if os.name == "nt":
                    subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], check=False)
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                process.wait()
            state["log"].close()
    (log_directory / "timings.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as output:
            output.write("\n| Native check lane | Seconds | Exit code |\n| --- | ---: | ---: |\n")
            for result in results:
                output.write(f"| {result['lane']} | {result['seconds']} | {result['exit_code']} |\n")
    return int(any(result["exit_code"] != 0 for result in results))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("phase", choices=["lint", "build", "test"])
    parser.add_argument("--parallel", type=int, required=True)
    arguments = parser.parse_args()
    def terminate(signum, _frame):
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, terminate)
    parallel = arguments.parallel
    if arguments.phase == "test":
        parallel = min(parallel * 2, os.cpu_count() or parallel)
    lanes = make_lanes(arguments.phase, parallel, dict(os.environ))
    return run_lanes(lanes, Path("build/ci-native-logs") / arguments.phase)


if __name__ == "__main__":
    raise SystemExit(main())
