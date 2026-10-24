#!/usr/bin/env python3
"""Deterministic whole-CLI context-volume benchmark; no live model inference.

The local Responses double returns fixed source-bound fixtures. The executable
must validate every report, including retained objections and an induced repair.
Text token counts use a named reference encoding, not vendor billed-token claims.
"""
import argparse
import copy
import hashlib
import importlib.metadata
import json
import os
import platform
import sys
from pathlib import Path
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROLES = ['constructor', 'sceptic', 'auditor', 'critic']


def compact(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'))


def digest(value):
    if not isinstance(value, str):
        value = compact(value)
    return hashlib.sha256(value.encode()).hexdigest()



def write_result(path, content):
    # A result path must never overwrite a retained input or another capture.
    with Path(path).open('x', encoding='utf-8') as output:
        output.write(content)


def tokenizer(name):
    try:
        import tiktoken
    except ImportError:
        return None, {'name': None, 'unavailable': 'tiktoken is not installed'}
    encoding = tiktoken.get_encoding(name)
    return encoding, {
        'package': 'tiktoken',
        'package_version': importlib.metadata.version('tiktoken'),
        'encoding': encoding.name,
        'scope': 'Named reference encoding of recorded text; model-specific framing, caching, hidden reasoning and billing are excluded.',
    }


def fixture(repo, workload):
    supplied = workload == 'supplied'
    case = json.loads((repo / 'examples' / ('case.json' if supplied else 'claim-only-case.json')).read_text())
    request = json.loads((repo / 'examples' / ('request.json' if supplied else 'claim-only-request.json')).read_text())
    count = 3 if workload in ['objections', 'repair'] else 0
    # Each specialist sees the same immutable bounded input. Their fixed outputs
    # intentionally share wording, exposing repeated context without claiming
    # independent empirical support or measuring actual model reasoning quality.
    assessment = {
        'findings': [
            'Conditional reasoning only. ' + case['warrants'][0]['rule'],
            'The available observations and the proposed conditions must retain their exact evidential status.',
        ],
        'objections': [
            'Hypothetical concern %s: %s Preserve this condition, its target, scope and proposed discriminating check.' % (i, case['defeaters'][0]['challenge'])
            for i in range(1, count + 1)
        ],
        'evidence_gaps': [case['evidence_plan'][0]['question']],
    }
    if count:
        # Long but bounded, unique context retained in every run. No claim here
        # is presented as empirical evidence by the fixture.
        for i in range(1, count + 1):
            assessment['findings'].append(
                'Conditional assessment %s. %s %s' % (i, case['warrants'][0]['applicability'], case['assumptions'][0]['statement'])
            )
    original = case['defeaters'][0]
    original_need = next(n for n in case['evidence_plan'] if n['target_id'] == original['id'])
    for role in ROLES:
        for i, challenge in enumerate(assessment['objections'], 1):
            objection = copy.deepcopy(original)
            objection.update(id=f'{role}-{i}', challenge=challenge)
            need = copy.deepcopy(original_need)
            need.update(id=f'need-{role}-{i}', target_id=objection['id'])
            case['defeaters'].append(objection)
            case['evidence_plan'].append(need)
    synthesis = copy.deepcopy(case)
    synthesis['defeaters'] = [d for d in synthesis['defeaters'] if not d['id'].startswith('critic-')]
    synthesis['evidence_plan'] = [n for n in synthesis['evidence_plan'] if not n['target_id'].startswith('critic-')]
    return request, case, synthesis, assessment


def metrics(report, bodies, encoding):
    records = []
    totals = {}
    for call in report['transcript']:
        values = {
            'instructions': call['prompt']['system'],
            'user': call['prompt']['user'],
            'schema': compact(call['prompt']['schema']),
            'output': call['response']['content'],
        }
        row = {'role': call['role'], 'components': {}}
        for name, text in values.items():
            data = {'utf8_bytes': len(text.encode()), 'reference_tokens': len(encoding.encode(text, disallowed_special=())) if encoding else None}
            row['components'][name] = data
            total = totals.setdefault(name, {'utf8_bytes': 0, 'reference_tokens': 0 if encoding else None})
            total['utf8_bytes'] += data['utf8_bytes']
            if encoding:
                total['reference_tokens'] += data['reference_tokens']
        row['response_sha256'] = digest(call['response']['content'])
        records.append(row)
    totals['input'] = {
        key: sum(totals[component][key] for component in ['instructions', 'user', 'schema']) if key != 'reference_tokens' or encoding else None
        for key in ['utf8_bytes', 'reference_tokens']
    }
    totals['input_plus_output'] = {
        key: totals['input'][key] + totals['output'][key] if key != 'reference_tokens' or encoding else None
        for key in ['utf8_bytes', 'reference_tokens']
    }
    return {
        'call_count': len(report['transcript']),
        'argument_sha256': digest(report['argument']),
        'request_sha256': digest(report['request']),
        'records': records,
        'totals': totals,
        'wire_request_utf8_bytes': sum(len(body) for body in bodies),
        'provider_usage': {'input_tokens': None, 'output_tokens': None, 'reason': 'No usage fabricated by replay provider.'},
    }


def run_one(binary, repo, workload, encoding):
    request, final_case, synthesis, assessment = fixture(repo, workload)
    repair = workload == 'repair'
    state = {'bodies': [], 'case_calls': 0, 'errors': []}
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            try:
                body = self.rfile.read(int(self.headers['Content-Length']))
                message = json.loads(body)
                if 'Authorization' in self.headers:
                    raise AssertionError('Benchmark must not send credentials')
                with lock:
                    state['bodies'].append(body)
                    is_case = 'current_claim' in message['text']['format']['schema']['properties']
                    if is_case:
                        state['case_calls'] += 1
                        case_number = state['case_calls']
                    else:
                        case_number = None
                if is_case:
                    content = copy.deepcopy(synthesis if case_number == 1 else final_case)
                    if repair and case_number == 2:
                        content['evidence_plan'] = [n for n in content['evidence_plan'] if n['target_id'] != 'auditor-3']
                else:
                    content = assessment
                text = compact(content)
                response = compact({'status': 'completed', 'model': 'deterministic-context-volume-replay', 'output': [{'type': 'message', 'content': [{'type': 'output_text', 'text': text}]}]}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(response)))
                self.end_headers()
                self.wfile.write(response)
            except Exception as exc:
                state['errors'].append(repr(exc))
                self.send_error(500)
        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='engarg-token-bench-') as directory:
            directory = Path(directory)
            source, output, audit = [directory / name for name in ['request.json', 'report.json', 'audit.jsonl']]
            source.write_text(compact(request))
            env = {key: value for key, value in os.environ.items() if key not in ['ENGARG_API_KEY', 'OPENAI_API_KEY', 'ENGARG_BENCH_KEY', 'ENGARG_REASONING_EFFORT', 'ENGARG_SERVICE_TIER']}
            command = [str(binary), 'analyse', '--input', str(source), '--model', 'benchmark-model', '--mode', 'hybrid', '--api', 'responses', '--endpoint', f'http://127.0.0.1:{server.server_port}/v1/responses', '--api-key-env', 'ENGARG_BENCH_KEY', '--argument-depth', '2', '--max-calls', '7' if repair else '6', '--max-repairs', '1' if repair else '0', '--max-response-bytes', '1048576', '--request-timeout-secs', '30', '--timeout-secs', '90', '--format', 'json', '--audit', str(audit), '--output', str(output)]
            started = time.perf_counter()
            result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=100)
            elapsed = time.perf_counter() - started
            if result.returncode != 0:
                raise RuntimeError(f'{workload} replay failed: {result.stderr}; server errors={state["errors"]}')
            report = json.loads(output.read_text())
            events = [json.loads(line) for line in audit.read_text().splitlines()]
            assert events[-1]['event'] == 'success'
            assert len(report['transcript']) == (7 if repair else 6)
            assert report['argument'] == final_case
            assert len(state['bodies']) == len(report['transcript'])
            assert len([e for e in events if e['event'] == 'completion']) == len(report['transcript'])
            data = metrics(report, state['bodies'], encoding)
            data['elapsed_seconds'] = elapsed
            data['accepted'] = True
            return data
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def replay_report(binary, source_path, encoding):
    """Replay exact recorded responses per role while rebuilding all prompts."""
    from collections import defaultdict, deque
    source_bytes = Path(source_path).read_bytes()
    original = json.loads(source_bytes)
    source_calls = original['transcript']
    config = original.get('run_config')
    if not config:
        raise ValueError('Retained replay requires an explicit original RunConfig')
    if config['mode'] != original['mode']:
        raise ValueError('Inconsistent source run mode')
    provider = original.get('provider_config') or {}
    if provider.get('api', 'Responses').lower() != 'responses':
        raise ValueError('Retained replay supports recorded Responses runs')
    responses = defaultdict(deque)
    clauses = {}
    for call in source_calls:
        if call.get('response_content_omitted'):
            raise ValueError('A source response was omitted and cannot be replayed')
        system = call['prompt']['system']
        if 'ROLE TASK: ' not in system:
            raise ValueError('Source has no ROLE TASK clause')
        clause = system.rsplit('ROLE TASK: ', 1)[1].split('\n\nCONSTRUCTION LIMIT:', 1)[0]
        role = call['role']
        if role in clauses and clauses[role] != clause:
            raise ValueError('Source role clauses differ within one role')
        clauses[role] = clause
        responses[role].append(call['response'])
    state = {'bodies': [], 'errors': [], 'replayed': defaultdict(list)}
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            try:
                body = self.rfile.read(int(self.headers['Content-Length']))
                message = json.loads(body)
                if 'Authorization' in self.headers:
                    raise AssertionError('Retained replay must not send credentials')
                system = message['instructions']
                if 'ROLE TASK: ' not in system:
                    raise ValueError('Request has no unchanged ROLE TASK clause')
                actual_clause = system.rsplit('ROLE TASK: ', 1)[1]
                matches = [role for role, clause in clauses.items() if actual_clause.startswith(clause)]
                if len(matches) != 1:
                    raise ValueError('Request does not identify exactly one retained role')
                role = matches[0]
                with lock:
                    if not responses[role]:
                        raise ValueError('Retained role response sequence exhausted')
                    recorded = responses[role].popleft()
                    state['bodies'].append(body)
                    state['replayed'][role].append(digest(recorded['content']))
                # Preserve recorded content and completion metadata verbatim.
                # Historical usage is replayed metadata, never newly billed use.
                envelope = {
                    'status': 'completed',
                    'model': recorded['model'],
                    'output': [{'type': 'message', 'content': [{'type': 'output_text', 'text': recorded['content']}]}],
                }
                if recorded.get('request_id') is not None:
                    envelope['id'] = recorded['request_id']
                if recorded.get('service_tier') is not None:
                    envelope['service_tier'] = recorded['service_tier']
                if recorded.get('reasoning_effort') is not None:
                    envelope['reasoning'] = {'effort': recorded['reasoning_effort']}
                if recorded.get('input_tokens') is not None or recorded.get('output_tokens') is not None:
                    envelope['usage'] = {key: recorded[key] for key in ['input_tokens', 'output_tokens'] if recorded.get(key) is not None}
                response = compact(envelope).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(response)))
                self.end_headers()
                self.wfile.write(response)
            except Exception as exc:
                state['errors'].append(type(exc).__name__)
                self.send_error(500)
        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with tempfile.TemporaryDirectory(prefix='engarg-retained-replay-') as directory:
            directory = Path(directory)
            source, output, audit = [directory / name for name in ['request.json', 'report.json', 'audit.jsonl']]
            source.write_text(compact(original['request']))
            key_name = 'ENGARG_BENCH_KEY'
            env = {key: value for key, value in os.environ.items() if key not in ['ENGARG_API_KEY', 'OPENAI_API_KEY', key_name, 'ENGARG_REASONING_EFFORT', 'ENGARG_SERVICE_TIER']}
            command = [str(binary), 'analyse', '--input', str(source), '--model', provider.get('default_model', 'benchmark-model'), '--mode', original['mode'], '--api', 'responses', '--endpoint', f'http://127.0.0.1:{server.server_port}/v1/responses', '--api-key-env', key_name, '--max-calls', str(config['max_calls']), '--max-repairs', str(config['max_repairs']), '--max-input-bytes', str(config['max_input_bytes']), '--max-response-bytes', str(config['max_response_bytes']), '--max-output-tokens', str(provider.get('max_output_tokens', 10000)), '--request-timeout-secs', str(provider.get('request_timeout_secs', 120)), '--timeout-secs', str(provider.get('timeout_secs', 480)), '--format', 'json', '--audit', str(audit), '--output', str(output)]
            if config.get('max_argument_depth') is not None:
                command += ['--argument-depth', str(config['max_argument_depth'])]
            if provider.get('requested_reasoning_effort') is not None:
                command += ['--reasoning-effort', provider['requested_reasoning_effort']]
            if provider.get('requested_service_tier') is not None:
                command += ['--service-tier', provider['requested_service_tier']]
            for role, model in provider.get('role_models', {}).items():
                command += ['--role-model', role + '=' + model]
            started = time.perf_counter()
            result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=120)
            elapsed = time.perf_counter() - started
            if result.returncode != 0:
                raise RuntimeError('Retained replay failed; exit=%s stderr_sha256=%s server_error_types=%s' % (result.returncode, digest(result.stderr), state['errors']))
            report = json.loads(output.read_text())
            events = [json.loads(line) for line in audit.read_text().splitlines()]
            assert events[-1]['event'] == 'success'
            assert report['request'] == original['request']
            assert report['argument'] == original['argument']
            assert report['mode'] == original['mode']
            assert report.get('run_config') == config
            assert len(report['transcript']) == len(source_calls)
            assert len(state['bodies']) == len(source_calls)
            assert len([e for e in events if e['event'] == 'completion']) == len(source_calls)
            assert not any(responses.values()), 'Retained source response left unused'
            for source_call, new_call in zip(source_calls, report['transcript']):
                assert new_call['role'] == source_call['role']
                assert new_call['response'] == source_call['response'], 'Recorded completion changed during replay'
            data = metrics(report, state['bodies'], encoding)
            data['elapsed_seconds'] = elapsed
            data['accepted'] = True
            data['source_report_sha256'] = hashlib.sha256(source_bytes).hexdigest()
            data['source_run_config_sha256'] = digest(config)
            data['source_response_sequence_sha256'] = digest([{'role': call['role'], 'response': call['response']} for call in source_calls])
            data['recorded_completions_preserved'] = True
            data['provider_usage']['reason'] = 'No new provider inference. Recorded historical completion metadata is replayed unchanged and excluded from candidate billed-use claims.'
            return data
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


def capture(args):
    repo = Path(args.repo).resolve()
    binary = Path(args.binary).resolve()
    encoding, identity = tokenizer(args.encoding)
    if args.repetitions < 1:
        raise ValueError('repetitions must be positive')
    workloads = args.workloads.split(',')
    if args.replay_report and 'retained_report' not in workloads:
        workloads.append('retained_report')
    if 'retained_report' in workloads and not args.replay_report:
        raise ValueError('retained_report workload requires --replay-report')
    data = {
        'benchmark': 'engarg.fixed-output-context-volume.v1',
        'label': args.label,
        'build_profile': args.build_profile,
        'python_version': sys.version.split()[0],
        'platform': platform.system() + ' ' + platform.machine(),
        'repository_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
        'working_tree_diff_sha256': digest(subprocess.check_output(['git', 'diff', '--binary', 'HEAD'], cwd=repo, text=True)),
        'source_tree_sha256': digest({str(path.relative_to(repo)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted([*repo.glob('src/**/*.rs'), repo / 'Cargo.toml', repo / 'Cargo.lock', repo / 'rust-toolchain.toml'])}),
        'benchmark_script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'binary': str(binary),
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'tokenizer': identity,
        'repetitions': args.repetitions,
        'workload_source': {name: digest((repo / 'examples' / name).read_text()) for name in ['case.json', 'claim-only-case.json', 'request.json', 'claim-only-request.json']},
        'limitations': [
            'Fixed outputs verify preserved structural acceptance, retained dissent and complete call path only.',
            'Reference text token counts exclude proprietary model framing, cached token classes, hidden reasoning and billing.',
            'Local replay latency is not live-model latency and cannot substantiate provider response-time savings.',
        ],
        'workloads': {},
    }
    for workload in workloads:
        operation = (lambda: replay_report(binary, args.replay_report, encoding)) if workload == 'retained_report' else (lambda: run_one(binary, repo, workload, encoding))
        data['workloads'][workload] = [operation() for _ in range(args.repetitions)]
    write_result(args.output, json.dumps(data, indent=2) + '\n')
    print(json.dumps({'output': str(Path(args.output).resolve()), 'label': args.label, 'workloads': {name: {'calls': rows[0]['call_count'], 'input_utf8_bytes': rows[0]['totals']['input']['utf8_bytes'], 'input_reference_tokens': rows[0]['totals']['input']['reference_tokens'], 'all_accepted': all(r['accepted'] for r in rows)} for name, rows in data['workloads'].items()}}, indent=2))


def compare(args):
    baseline, candidate = [json.loads(Path(path).read_text()) for path in [args.baseline, args.candidate]]
    assert baseline['tokenizer'] == candidate['tokenizer'], 'Use identical reference tokenizer'
    assert baseline['workloads'].keys() == candidate['workloads'].keys(), 'Use identical workloads'
    assert baseline['workload_source'] == candidate['workload_source'], 'Use identical immutable replay source'
    result = {'baseline': baseline['label'], 'candidate': candidate['label'], 'tokenizer': baseline['tokenizer'], 'workloads': {}}
    for workload, base_runs in baseline['workloads'].items():
        new_runs = candidate['workloads'][workload]
        assert len(base_runs) == len(new_runs)
        assert all(row['accepted'] for row in base_runs + new_runs)
        assert all(b['argument_sha256'] == c['argument_sha256'] and b['request_sha256'] == c['request_sha256'] and b['call_count'] == c['call_count'] for b, c in zip(base_runs, new_runs))
        assert all([(r['role'], r['response_sha256']) for r in b['records']] == [(r['role'], r['response_sha256']) for r in c['records']] for b, c in zip(base_runs, new_runs)), 'Fixed completion sequence changed'
        if workload == 'retained_report':
            assert all(b['source_report_sha256'] == c['source_report_sha256'] and b['source_response_sequence_sha256'] == c['source_response_sequence_sha256'] and b['source_run_config_sha256'] == c['source_run_config_sha256'] for b, c in zip(base_runs, new_runs)), 'Retained source identity changed'
        components = {}
        for component in base_runs[0]['totals']:
            changes = {}
            for unit in ['utf8_bytes', 'reference_tokens']:
                old = [row['totals'][component][unit] for row in base_runs]
                new = [row['totals'][component][unit] for row in new_runs]
                if None in old + new:
                    changes[unit] = None
                else:
                    assert len(set(old)) == 1 and len(set(new)) == 1, 'Deterministic replay volume varied'
                    changes[unit] = {'baseline': old[0], 'candidate': new[0], 'absolute_reduction': old[0] - new[0], 'percent_reduction': 100 * (old[0] - new[0]) / old[0] if old[0] else None, 'baseline_range': [min(old), max(old)], 'candidate_range': [min(new), max(new)]}
            components[component] = changes
        result['workloads'][workload] = {'calls': base_runs[0]['call_count'], 'argument_and_request_identical': True, 'components': components}
    text = json.dumps(result, indent=2) + '\n'
    if args.output:
        write_result(args.output, text)
    print(text)



def document(args):
    """Measure the optional audit appendix against an identical complete case."""
    binary = Path(args.binary).resolve()
    source = Path(args.input).resolve()
    original = source.read_bytes()
    command = [str(binary), 'render', '--input', str(source), '--format', 'markdown']
    full = subprocess.check_output(command, timeout=30)
    brief = subprocess.check_output(command + ['--no-audit-appendix'], timeout=30)
    marker = b'\n## Complete audit record\n\n'
    assert full.rsplit(marker, 1)[0].rstrip() == brief.rstrip()
    assert source.read_bytes() == original
    encoding, identity = tokenizer(args.encoding)
    result = {
        'benchmark': 'engarg.audit-appendix-context.v1',
        'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'source_report_sha256': hashlib.sha256(original).hexdigest(),
        'tokenizer': identity,
        'canonical_source_unchanged': True,
        'all_argument_records_and_registers_identical': True,
        'utf8_bytes': {'full': len(full), 'without_audit_appendix': len(brief)},
        'reference_tokens': {'full': len(encoding.encode(full.decode(), disallowed_special=())), 'without_audit_appendix': len(encoding.encode(brief.decode(), disallowed_special=()))} if encoding else None,
        'limitation': 'Downstream document context only; this option does not reduce the inference used to construct the argument.',
    }
    write_result(args.output, json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('capture')
    p.add_argument('--binary', required=True)
    p.add_argument('--repo', required=True)
    p.add_argument('--label', required=True)
    p.add_argument('--build-profile', choices=['release', 'debug', 'unknown'], default='unknown')
    p.add_argument('--output', required=True)
    p.add_argument('--encoding', default='o200k_base')
    p.add_argument('--replay-report', help='Optional private retained RunReport; outputs only digests and volume metrics')
    p.add_argument('--repetitions', type=int, default=3)
    p.add_argument('--workloads', default='claim_only,supplied,objections,repair')
    p.set_defaults(func=capture)
    p = sub.add_parser('compare')
    p.add_argument('baseline')
    p.add_argument('candidate')
    p.add_argument('--output')
    p.set_defaults(func=compare)
    p = sub.add_parser('document')
    p.add_argument('--binary', required=True)
    p.add_argument('--input', required=True)
    p.add_argument('--output', required=True)
    p.add_argument('--encoding', default='o200k_base')
    p.set_defaults(func=document)
    args = parser.parse_args()
    args.func(args)


if __name__ == '__main__':
    main()
