"""Read bounded operator JSON and preserve decimal token spellings on output."""

import json
import os
import stat


class ExactNumber(str):
    """A JSON decimal token retained without a binary floating-point conversion."""


def exact_json(value):
    """Serialise admitted JSON while preserving every ExactNumber token."""
    if isinstance(value, ExactNumber):
        return str(value)
    if isinstance(value, dict):
        return '{' + ','.join(
            json.dumps(key, ensure_ascii=False) + ':' + exact_json(item)
            for key, item in value.items()
        ) + '}'
    if isinstance(value, list):
        return '[' + ','.join(map(exact_json, value)) + ']'
    return json.dumps(value, ensure_ascii=False, allow_nan=False)


def unique(pairs):
    """Reject duplicate object keys instead of silently replacing their values."""
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('Duplicate JSON key')
        result[key] = value
    return result


def _reject_constant(value):
    raise ValueError('Non-finite JSON number')


def read(path, limit=1_048_576):
    """Read a regular file once and return its exact bytes and admitted object."""
    flags = os.O_RDONLY | os.O_NONBLOCK | getattr(os, 'O_NOFOLLOW', 0)
    with os.fdopen(os.open(path, flags), 'rb') as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise ValueError('Operator inputs require regular files')
        raw = source.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('Operator input exceeds its byte limit')
    value = json.loads(
        raw, object_pairs_hook=unique, parse_float=ExactNumber,
        parse_constant=_reject_constant,
    )
    if not isinstance(value, dict):
        raise ValueError('Operator inputs require JSON objects')
    return raw, value
