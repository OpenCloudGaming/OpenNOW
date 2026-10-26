import os
from pathlib import Path
import subprocess
import threading
import time

destination = Path('build/intel-controller-profiles')
destination.mkdir(parents=True, exist_ok=True)
binary = 'build/probe/opennow-controllerinput-tests'
for mode, overrides in [
    ('native', {}),
    ('virtual', {'SDL_JOYSTICK_IOKIT': '0', 'SDL_JOYSTICK_MFI': '0', 'SDL_JOYSTICK_HIDAPI': '0'}),
]:
    started = time.monotonic()
    with (destination / (mode + '.log')).open('w') as log:
        process = subprocess.Popen([binary, '-v2'], env=os.environ | overrides,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        def profile():
            for index in range(3):
                time.sleep(6)
                if process.poll() is not None:
                    return
                subprocess.run(['sample', str(process.pid), '1', '1', '-file',
                                str(destination / f'{mode}-{index}.sample')], check=False)
        sampler = threading.Thread(target=profile, daemon=True)
        sampler.start()
        def stop():
            if process.poll() is None:
                process.kill()
        deadline = threading.Timer(30, stop)
        deadline.start()
        for line in process.stdout:
            message = f'{mode} {time.monotonic()-started:.3f}s {line}'
            log.write(message)
            log.flush()
            print(message, end='', flush=True)
        process.wait()
        deadline.cancel()
        sampler.join()
        print(mode, 'EXIT', process.returncode, 'WALL', time.monotonic()-started, flush=True)
