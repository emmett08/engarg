"""Select explicit collector variable names across the isolated EAL host boundary."""

import os
from pathlib import Path
import re

from .json_io import read


# These public process defaults already cross eal_host::command. Forwarding
# them again as collector-env would incorrectly classify their values as secrets.
HOST_DEFAULT_ENVIRONMENT = {
    'PATH', 'HOME', 'USER', 'LOGNAME', 'TMPDIR', 'TMP', 'TEMP', 'LANG', 'LC_ALL',
    'SSL_CERT_FILE', 'SSL_CERT_DIR', 'SYSTEMROOT', 'WINDIR', 'PATHEXT',
}


def selected_names(bundle, environment):
    """Return present extra names authorised by the graph's emitted registry."""
    if bundle.get('schema_version') != 'engarg.support-export/1':
        raise ValueError('Collector environment requires a graph export bundle')
    registry = bundle.get('registry')
    tools = registry.get('tools') if isinstance(registry, dict) else None
    if not isinstance(tools, dict):
        raise ValueError('Graph export requires its emitted tool registry')
    names = set()
    for binding in tools.values():
        inherited = binding.get('inherit_env') if isinstance(binding, dict) else None
        if not isinstance(inherited, list) or any(
                not isinstance(name, str) or len(name) > 1024
                or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', name)
                for name in inherited):
            raise ValueError('Collector registry has invalid environment variable names')
        names.update(inherited)
    selected = sorted(name for name in names - HOST_DEFAULT_ENVIRONMENT if environment.get(name))
    if len(selected) > 64:
        raise ValueError('Collector environment exceeds 64 variable names')
    return selected


def print_names(bundle_path):
    """Print names only; the host reads values and checks the actual TOML registry."""
    _, bundle = read(Path(bundle_path), limit=16_777_216)
    for name in selected_names(bundle, os.environ):
        print(name)
