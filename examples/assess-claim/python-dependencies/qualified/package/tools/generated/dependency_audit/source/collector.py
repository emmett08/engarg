import sys
import json
import os
import re
from pathlib import Path
from collections import deque
from datetime import datetime, timezone
from email.parser import Parser
from email.policy import compat32
from importlib.metadata import distributions

import packaging
from packaging.requirements import Requirement
from packaging.version import Version
from packaging.utils import canonicalize_name
from packaging.markers import default_environment


class AcquisitionError(Exception):
    pass


class NumberToken(str):
    """An opaque JSON number; never converted through a floating-point value."""


def fail(message):
    raise AcquisitionError(message)


def pairs_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            fail('duplicate JSON object key')
        result[key] = value
    return result


def invalid_constant(value):
    fail('invalid JSON constant')


def read_request():
    limit = 8 * 1024 * 1024
    data = sys.stdin.buffer.read(limit + 1)
    if len(data) > limit:
        fail('request exceeds transport bound')
    text = data.decode('utf-8', errors='strict')
    decoder = json.JSONDecoder(
        object_pairs_hook=pairs_object,
        parse_int=NumberToken,
        parse_float=NumberToken,
        parse_constant=invalid_constant,
    )

    def whitespace(position):
        while position < len(text) and text[position] in ' \r\n\t':
            position += 1
        return position

    position = whitespace(0)
    if position >= len(text) or text[position] != '{':
        fail('request must be an object')
    position = whitespace(position + 1)
    request = {}
    raw = {}
    if position < len(text) and text[position] == '}':
        position += 1
    else:
        while True:
            key, end = decoder.raw_decode(text, position)
            if type(key) is not str or key in request:
                fail('invalid or duplicate request key')
            position = whitespace(end)
            if position >= len(text) or text[position] != ':':
                fail('missing JSON colon')
            position = whitespace(position + 1)
            start = position
            value, end = decoder.raw_decode(text, position)
            request[key] = value
            raw[key] = text[start:end]
            position = whitespace(end)
            if position >= len(text):
                fail('unterminated request')
            separator = text[position]
            position = whitespace(position + 1)
            if separator == '}':
                break
            if separator != ',':
                fail('invalid JSON separator')
    if whitespace(position) != len(text):
        fail('trailing input')
    expected = {'evidence_id', 'environment', 'tool', 'tool_version', 'input', 'context'}
    if set(request) != expected:
        fail('incorrect request fields')
    for key in ('evidence_id', 'environment', 'tool', 'tool_version'):
        if type(request[key]) is not str:
            fail('request identity fields must be strings')
    if not request['evidence_id']:
        fail('empty evidence identity')
    if type(request['input']) is not dict or type(request['context']) is not dict:
        fail('input and context must be objects')
    arguments = request['input']
    if set(arguments) != {'image_id', 'metadata_directory', 'roots'}:
        fail('incorrect input fields')
    if type(arguments['image_id']) is not str or type(arguments['metadata_directory']) is not str:
        fail('invalid input string')
    roots = arguments['roots']
    if type(roots) is not list or not 1 <= len(roots) <= 32:
        fail('invalid roots array')
    if any(type(root) is not str for root in roots):
        fail('root requirements must be strings')
    return request, raw


class Budget:
    def __init__(self):
        self.used = 0

    def charge(self):
        if self.used >= 20000:
            fail('processing budget exhausted')
        self.used += 1


def parse_requirement(text, budget):
    budget.charge()
    requirement = Requirement(text)
    if requirement.url is not None:
        fail('direct URL requirements are not supported')
    canonicalize_name(requirement.name, validate=True)
    return requirement


def scoped_path(path, directory):
    resolved = path.resolve(strict=True)
    if not resolved.is_relative_to(directory):
        fail('metadata escapes the selected directory')
    return resolved


def read_metadata(distribution, directory):
    # PathDistribution.read_text deliberately suppresses important I/O errors.
    # Use its discovered metadata location, but perform strict reads ourselves.
    location = getattr(distribution, '_path', None)
    if location is None or not isinstance(location, Path):
        fail('unsupported metadata source')
    location = scoped_path(location, directory)
    if not location.is_dir():
        fail('distribution metadata location is not a directory')
    primary = location / 'METADATA'
    try:
        primary.lstat()
    except FileNotFoundError:
        source = location / 'PKG-INFO'
    else:
        source = primary
    source = scoped_path(source, directory)
    with source.open('rb') as stream:
        text = stream.read().decode('utf-8', errors='strict')
    if '\x00' in text:
        fail('NUL in metadata')
    message = Parser(policy=compat32).parsestr(text)
    if message.defects:
        fail('malformed metadata')
    for key in message.keys():
        if not re.fullmatch(r'[!-9;-~]+', key, flags=re.ASCII):
            fail('malformed metadata header')
    names = message.get_all('Name', [])
    versions = message.get_all('Version', [])
    if len(names) != 1 or len(versions) != 1:
        fail('metadata requires exactly one Name and Version')
    name, version = names[0], versions[0]
    if not name.strip() or not version.strip():
        fail('empty Name or Version')
    canonical = canonicalize_name(name, validate=True)
    parsed_version = Version(version)
    requirements = message.get_all('Requires-Dist', [])
    return canonical, name, version, parsed_version, requirements


def audit(arguments):
    if packaging.__version__ != '26.3':
        fail('packaging 26.3 is required')
    directory = Path(arguments['metadata_directory'])
    # Relative supplied fixture paths are relative to the mounted data layout.
    # Absolute installed-metadata paths are used as supplied.
    if not directory.is_absolute():
        directory = Path('/fixtures') / directory
    directory = directory.resolve(strict=True)
    if not directory.is_dir():
        fail('metadata_directory must be a directory')
    # Explicitly exhaust a directory read: importlib can suppress directory errors.
    with os.scandir(directory) as entries:
        for entry in entries:
            entry.name

    budget = Budget()
    roots = [parse_requirement(text, budget) for text in arguments['roots']]
    inventory = {}
    for distribution in distributions(path=[str(directory)]):
        if len(inventory) >= 512:
            fail('distribution count exceeds bound')
        canonical, name, version, parsed_version, texts = read_metadata(distribution, directory)
        if canonical in inventory:
            fail('ambiguous duplicate canonical distribution name')
        parsed = [parse_requirement(text, budget) for text in texts]
        inventory[canonical] = {
            'name': name,
            'version': version,
            'parsed_version': parsed_version,
            'texts': texts,
            'requirements': parsed,
        }

    environment = default_environment()
    queue = deque()
    scheduled = set()
    reachable = set()
    active = set()

    def seed(requirement):
        target = canonicalize_name(requirement.name)
        reachable.add(target)
        contexts = [''] + sorted({canonicalize_name(extra) for extra in requirement.extras})
        for extra in contexts:
            state = (target, extra)
            if state not in scheduled:
                scheduled.add(state)
                queue.append(state)

    for requirement in roots:
        seed(requirement)
    while queue:
        name, extra = queue.popleft()
        record = inventory.get(name)
        if record is None:
            continue
        marker_environment = dict(environment, extra=extra)
        for index, requirement in enumerate(record['requirements']):
            if requirement.marker is None:
                enabled = True
            else:
                budget.charge()
                enabled = requirement.marker.evaluate(environment=marker_environment)
            occurrence = (name, index)
            if enabled and occurrence not in active:
                active.add(occurrence)
                seed(requirement)

    violations = []
    root_versions = []
    checked = 0

    def check(requiring, index, text, requirement):
        nonlocal checked
        budget.charge()
        checked += 1
        target = canonicalize_name(requirement.name)
        record = inventory.get(target)
        observed = None if record is None else record['version']
        if record is None:
            reason = 'missing'
        elif not requirement.specifier.contains(record['parsed_version'], prereleases=None):
            reason = 'incompatible_version'
        else:
            return observed
        violations.append({
            'requiring_package': requiring,
            'requirement_index': index,
            'constraint': text,
            'target': target,
            'observed_version': observed,
            'reason': reason,
        })
        return observed

    for index, requirement in enumerate(roots):
        text = arguments['roots'][index]
        observed = check('<root>', index, text, requirement)
        root_versions.append({
            'requirement': text,
            'canonical_name': canonicalize_name(requirement.name),
            'observed_version': observed,
        })
    skipped = []
    for name in sorted(reachable):
        record = inventory.get(name)
        if record is None:
            continue
        for index, requirement in enumerate(record['requirements']):
            text = record['texts'][index]
            if (name, index) in active:
                check(name, index, text, requirement)
            else:
                skipped.append({
                    'requiring_package': name,
                    'requirement_index': index,
                    'requirement': text,
                })
    violations.sort(key=lambda item: (
        item['requiring_package'], item['requirement_index'], item['constraint'], item['target']))
    skipped.sort(key=lambda item: (item['requiring_package'], item['requirement_index']))
    records = [{
        'canonical_name': name,
        'Name': inventory[name]['name'],
        'Version': inventory[name]['version'],
        'Requires-Dist': inventory[name]['texts'],
    } for name in sorted(inventory)]
    return {
        'collected': True,
        'satisfied': not violations,
        'inventory_complete': True,
        'violations_count': len(violations),
        'checked_distributions': len(inventory),
        'checked_requirements': checked,
        'details': {
            'roots': arguments['roots'],
            'root_versions': root_versions,
            'inventory': records,
            'violations': violations,
            'skipped_requirements': skipped,
        },
    }


def main():
    request, raw = read_request()
    value = audit(request['input'])
    observed_at = datetime.now(timezone.utc).isoformat(timespec='microseconds').replace('+00:00', 'Z')

    def encode(item):
        return json.dumps(item, ensure_ascii=True, allow_nan=False, separators=(',', ':'))

    # Raw opaque JSON fragments are emitted directly, without numeric conversion
    # or special interpretation of any object keys.
    output = (
        '{"value":' + encode(value)
        + ',"observed_at":' + encode(observed_at)
        + ',"context":' + raw['context']
        + ',"request":{"tool":' + encode(request['tool'])
        + ',"tool_version":' + encode(request['tool_version'])
        + ',"input":' + raw['input']
        + ',"context":' + raw['context'] + '}}\n'
    ).encode('utf-8', errors='strict')
    if len(output) > 1048576:
        fail('response exceeds transport bound')
    sys.stdout.buffer.write(output)
    sys.stdout.buffer.flush()


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print('dependency audit acquisition failed: ' + type(error).__name__, file=sys.stderr)
        sys.exit(1)
