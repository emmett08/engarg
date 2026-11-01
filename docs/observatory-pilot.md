# Linked execution and effort pilot

This pilot records EngArg's Python authoring-script checks in both `checks`
matrix jobs. Each job configures the pinned Observatory action, wraps
`python3 -m unittest discover -s tests -p 'test_*.py'`, and publishes its own
immutable receipt bundle. The remaining CI commands are outside this wrapper's
observation scope.

| Identity | Value |
| --- | --- |
| Protocol | `observatory-linked-pilot-v1` |
| Task | `engarg-authoring-effort-pilot` |
| Sample issue | [#49](https://github.com/emmett08/engarg/issues/49) |
| Implementation PR | [#50](https://github.com/emmett08/engarg/pull/50) |
| Implementation branch | `feat/observatory-effort-pilot` |
| Observatory revision | `38f022d2d2f6e015fa42a23f1bc335783e263708` |

PR #50 closes #49. These exact protocol and task IDs must also appear in the
private effort configuration. Observatory's `sync-links` command can validate
the closing-issue relationship and retain the actual PR number and branch.
This is the relevant configuration fragment; add
real participant windows and device bucket IDs privately before collecting:

```json
{
  "protocol_id": "observatory-linked-pilot-v1",
  "links": [
    {
      "task_id": "engarg-authoring-effort-pilot",
      "repository": "emmett08/engarg",
      "issue_number": 49,
      "branches": ["feat/observatory-effort-pilot"],
      "pull_requests": [50],
      "projects": ["engarg"]
    }
  ]
}
```

This fragment is not a complete runnable configuration. Confirm that `engarg`
is the actual project label emitted by the participant's context watcher. A
GitHub branch or account alone does not establish who controlled a tool or how
much human attention was spent on it.

The fixed task ID identifies this bounded instrumentation pilot. For unrelated
future work, configure the actual issue/task mapping before treating receipts
as evidence for that work; this sample issue is not its requirement.

## Evidence and delivery

The action separately retains GitHub's original actor, rerun actor and trigger
event. The declared controller is `unknown`, with no invented assistant session.
The job display name and allowlisted operating-system matrix value support
matching receipts to independently collected GitHub jobs. Reruns and retried
commands retain distinct run attempts and invocation identities.

Publication runs immediately after the selected check, including when that
check fails. Runner loss can still prevent delivery. The private collector must
reconcile completed runs, attempts and jobs and retain missing uploads instead
of treating them as successful zero-duration executions. Artifact retention is
30 days; collect bundles before they expire.

## Human effort

The CI receipts measure the selected tool's execution. Human-effort estimation
requires the private activity collection and sampled-response process described
in [Observatory's effort guide](https://github.com/emmett08/engineering-work-observatory/blob/38f022d2d2f6e015fa42a23f1bc335783e263708/docs/effort-estimation.md).
Use the same task identity when collecting activity, sampling intervals and
reporting aggregates. Missing responses or an incomplete declared observation
frame leave the human estimate unavailable. CI duration is never converted into
human hours.

The **Optional effort observation** issue form records an occasional aggregate
self-report. It is separate from the randomly selected, hash-bound private
response forms; submitting the issue form does not create a calibrated estimate
or enter the estimator automatically. For a sampled response, use the private
form for the selected interval instead. Do not post response files or raw
activity logs in this public repository.

The collector can publish an aggregate execution/effort report to #49 or the
linked PR after collection and validation. An unavailable human estimate is a
valid demonstration result; simulated observations must remain labelled as
synthetic.
