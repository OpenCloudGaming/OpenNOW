import os
from pathlib import Path
import subprocess
import threading
import time

destination = Path('build/intel-controller-profiles')
destination.mkdir(parents=True, exist_ok=True)
tests = ['controllerinput', 'controllersources', 'controllernavigation', 'controllertuning', 'controllermetadata', 'generichidinput']
pattern = '^opennow-(' + '|'.join(tests) + ')-tests$'
registration = Path('build/probe/CTestTestfile.cmake')
for mode in ['parallel']:
    if mode == 'locked':
        with registration.open('a') as output:
            output.write('\nset_tests_properties(' + ' '.join('opennow-' + name + '-tests' for name in tests) + ' PROPERTIES RESOURCE_LOCK native_controller_discovery)\n')
    with (destination / (mode + '.log')).open('w') as log:
        result = subprocess.run(['ctest', '--test-dir', 'build/probe', '--parallel', '2', '--output-on-failure', '--repeat', 'until-fail:10', '-R', pattern], stdout=log, stderr=subprocess.STDOUT)
        print(mode, 'EXIT', result.returncode, flush=True)
    print((destination / (mode + '.log')).read_text(), flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
