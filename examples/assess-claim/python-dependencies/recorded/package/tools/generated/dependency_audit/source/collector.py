import sys
import json
import os
from pathlib import Path
from collections import deque
from datetime import datetime, timezone
from email.parser import Parser
from email import policy
from importlib import metadata

import packaging
from packaging.requirements import Requirement
from packaging.version import Version
from packaging.utils import canonicalize_name
from packaging.markers import default_environment


class AcquisitionError(Exception):
    pass


class Number(str):
    pass


def fail(message):
    raise AcquisitionError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            fail('Duplicate JSON key')
        result[key] = value
    return result


def reject_constant(value):
    fail('Invalid JSON constant')


def encode(value):
    if isinstance(value, Number):
        return str(value)
    if value is None:
        return 'null'
    if value is True:
        return 'true'
    if value is False:
        return 'false'
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=True)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, list):
        return '[' + ','.join(encode(item) for item in value) + ']'
    if isinstance(value, dict):
        return '{' + ','.join(json.dumps(key, ensure_ascii=True) + ':' + encode(item)
                              for key, item in value.items()) + '}'
    fail('Unsupported JSON value')


def is_string(value):
    return isinstance(value, str) and not isinstance(value, Number)


def validate_request(request):
    if not isinstance(request, dict) or set(request) != {
        'evidence_id', 'environment', 'tool', 'tool_version', 'input', 'context'
    }:
        fail('Invalid request fields')
    for field in ('evidence_id', 'environment', 'tool', 'tool_version'):
        if not is_string(request[field]):
            fail('Invalid request string')
    if not request['evidence_id']:
        fail('Empty evidence identity')
    if not isinstance(request['context'], dict):
        fail('Context must be an object')
    args = request['input']
    if not isinstance(args, dict) or set(args) != {'image_id', 'metadata_directory', 'roots'}:
        fail('Invalid input fields')
    if not is_string(args['image_id']) or not is_string(args['metadata_directory']):
        fail('Invalid input string')
    roots = args['roots']
    if not isinstance(roots, list) or not 1 <= len(roots) <= 32:
        fail('Invalid roots array')
    if not all(is_string(root) for root in roots):
        fail('Invalid root requirement')
    return args


class Budget:
    def __init__(self):
        self.used = 0

    def charge(self):
        if self.used >= 20000:
            fail('Processing budget exhausted')
        self.used += 1


def parse_requirement(text, budget):
    budget.charge()
    req = Requirement(text)
    if req.url is not None:
        fail('Direct URL requirements are not supported')
    canonicalize_name(req.name, validate=True)
    return req


def read_distribution(dist):
    # importlib's convenience reader suppresses some filesystem exceptions.
    # Read its selected metadata location explicitly so those remain errors.
    location = Path(dist._path)
    if location.is_dir():
        filename = location / 'METADATA'
        if not filename.exists():
            filename = location / 'PKG-INFO'
    else:
        filename = location
    with filename.open('r', encoding='utf-8', errors='strict') as stream:
        raw = stream.read()
    message = Parser(policy=policy.compat32).parsestr(raw)
    if message.defects:
        fail('Malformed distribution metadata')
    for key, value in message.items():
        if not isinstance(value, str):
            fail('Malformed metadata header')
    names = message.get_all('Name', [])
    versions = message.get_all('Version', [])
    if len(names) != 1 or len(versions) != 1:
        fail('Missing or repeated Name or Version')
    name, version = names[0], versions[0]
    if not name.strip() or not version.strip():
        fail('Empty Name or Version')
    canonical = canonicalize_name(name, validate=True)
    parsed_version = Version(version)
    requirements = message.get_all('Requires-Dist', [])
    return canonical, name, version, parsed_version, requirements


def audit(args):
    if packaging.__version__ != '26.3':
        fail('The configured operation requires packaging 26.3')
    budget = Budget()
    roots = [parse_requirement(text, budget) for text in args['roots']]
    directory_text = args['metadata_directory']
    if not directory_text or '\x00' in directory_text:
        fail('Invalid metadata directory')
    directory = Path(directory_text)
    if not directory.is_absolute():
        if '..' in directory.parts:
            fail('Relative metadata directory escapes fixture layout')
        directory = Path('/fixtures') / directory
    if not directory.is_dir():
        fail('Metadata directory does not exist')
    # Force directory enumeration errors to propagate before importlib discovery.
    with os.scandir(directory) as entries:
        for entry in entries:
            entry.stat()
    inventory = {}
    for dist in metadata.distributions(path=[str(directory)]):
        if len(inventory) >= 512:
            fail('Distribution limit exceeded')
        canonical, name, version, parsed_version, fields = read_distribution(dist)
        if canonical in inventory:
            fail('Ambiguous duplicate distribution name')
        parsed = [parse_requirement(field, budget) for field in fields]
        inventory[canonical] = {
            'record': {'canonical_name': canonical, 'Name': name,
                       'Version': version, 'Requires-Dist': fields},
            'version': parsed_version,
            'requirements': parsed,
        }

    pending = deque()
    states = set()
    reachable = set()
    active = set()

    def enable(req):
        target = canonicalize_name(req.name)
        reachable.add(target)
        contexts = [''] + sorted({canonicalize_name(extra) for extra in req.extras})
        for extra in contexts:
            state = (target, extra)
            if state not in states:
                states.add(state)
                pending.append(state)

    for root in roots:
        enable(root)
    environment = default_environment()
    while pending:
        package, extra = pending.popleft()
        installed = inventory.get(package)
        if installed is None:
            continue
        marker_environment = dict(environment, extra=extra)
        for index, req in enumerate(installed['requirements']):
            if req.marker is None:
                applies = True
            else:
                budget.charge()
                applies = req.marker.evaluate(environment=marker_environment)
            occurrence = (package, index)
            if applies and occurrence not in active:
                active.add(occurrence)
                enable(req)

    violations = []
    skipped = []
    root_versions = []
    checked = 0

    def observed(target):
        entry = inventory.get(target)
        return None if entry is None else entry['record']['Version']

    def check(requiring, index, text, req):
        nonlocal checked
        budget.charge()
        checked += 1
        target = canonicalize_name(req.name)
        entry = inventory.get(target)
        reason = None
        if entry is None:
            reason = 'missing'
        elif not req.specifier.contains(entry['version'], prereleases=None):
            reason = 'incompatible_version'
        if reason is not None:
            violations.append({
                'requiring_package': requiring,
                'requirement_index': index,
                'constraint': text,
                'target': target,
                'observed_version': observed(target),
                'reason': reason,
            })

    for index, req in enumerate(roots):
        text = args['roots'][index]
        target = canonicalize_name(req.name)
        root_versions.append({'requirement': text, 'canonical_name': target,
                              'observed_version': observed(target)})
        check('<root>', index, text, req)
    for package in sorted(reachable):
        entry = inventory.get(package)
        if entry is None:
            continue
        for index, req in enumerate(entry['requirements']):
            text = entry['record']['Requires-Dist'][index]
            if (package, index) in active:
                check(package, index, text, req)
            else:
                skipped.append({'requiring_package': package,
                                'requirement_index': index, 'requirement': text})
    violations.sort(key=lambda v: (v['requiring_package'], v['requirement_index'],
                                   v['constraint'], v['target']))
    skipped.sort(key=lambda v: (v['requiring_package'], v['requirement_index']))
    return {
        'collected': True,
        'satisfied': not violations,
        'inventory_complete': True,
        'violations_count': len(violations),
        'checked_distributions': len(inventory),
        'checked_requirements': checked,
        'details': {
            'roots': args['roots'], 'root_versions': root_versions,
            'violations': violations, 'skipped_requirements': skipped,
            'inventory': [inventory[name]['record'] for name in sorted(inventory)],
        },
    }


def main():
    limit = 8 * 1024 * 1024
    data = sys.stdin.buffer.read(limit + 1)
    if len(data) > limit:
        fail('Request exceeds size bound')
    request = json.loads(data.decode('utf-8'), parse_int=Number, parse_float=Number,
                         parse_constant=reject_constant, object_pairs_hook=unique_object)
    args = validate_request(request)
    value = audit(args)
    observed_at = datetime.now(timezone.utc).isoformat(timespec='microseconds').replace('+00:00', 'Z')
    response = {
        'value': value,
        'observed_at': observed_at,
        'context': request['context'],
        'request': {key: request[key] for key in ('tool', 'tool_version', 'input', 'context')},
    }
    output = encode(response)
    if len(output.encode('utf-8')) > 1048576:
        fail('Response exceeds output bound')
    sys.stdout.write(output + '\n')


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        sys.stderr.write('Collector failed: ' + type(exc).__name__ + ': ' + str(exc) + '\n')
        sys.exit(1)
