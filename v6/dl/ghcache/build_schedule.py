#!/usr/bin/env python3
"""Rebuild ghcache.schedule.json against the emitted program.

The `headers` column is a `json_group_object` the PROGRAM builds, so its exact
text is not guessable: the demand row is read back out of the fold after each
batch and the scripted response is written to match. Run from this directory
with the emitted program as the one argument.
"""
import json
import subprocess
import sys

HERE = __file__.rsplit("/", 1)[0]
HARNESS = f"{HERE}/../../sprefa-engine-rs/target/debug/emit_rust_harness"


def clock(every, ordinal, bucket):
    return {
        "rel": "__host_response_clock__tick",
        "sign": "add",
        "row": [f"witness|clock__tick|every:int={every}", ordinal, every, bucket],
    }


CONFIG = {
    "global": {
        "poll_interval_seconds": 60,
        "org_repo_discovery_interval_seconds": 3600,
        "branches_poll_interval_seconds": 60,
        "rate_warn_threshold": 200,
        "rate_stop_threshold": 100,
        "warn_stretch_multiplier": 2,
        "heartbeat_ttl_seconds": 0,
        "sync_notifications": 0,
        "staging_folder": "",
    },
    "repo": [
        {
            "owner": "acme",
            "name": "widgets",
            "default_branch": "main",
            "sync_prs": 1,
            "sync_events": 1,
            "sync_notifications": 0,
            "sync_branches": "",
            "checkout_on_sync": 0,
            "checkout_pr_branches": 0,
        }
    ],
}

EVENTS = json.dumps(
    [
        {
            "id": "1",
            "type": "PushEvent",
            "actor": {"login": "someone"},
            "payload": {"ref": "refs/heads/main", "action": "pushed"},
            "created_at": "2026-08-22T00:00:00Z",
        }
    ],
    separators=(",", ":"),
)

# A second, distinct event: `dirty_repo` derives from a FRESH `repo_event_seen`
# decode, so bucket 3's re-poll of `pr_due` needs a 200 here, not another 304.
EVENTS_2 = json.dumps(
    [
        {
            "id": "2",
            "type": "PullRequestEvent",
            "actor": {"login": "someone"},
            "payload": {"action": "closed", "number": 1},
            "created_at": "2026-08-23T00:10:00Z",
        }
    ],
    separators=(",", ":"),
)


def demands(program, batches, rel="__host_demand_http__get"):
    schedule = "/tmp/ghcache-schedule-step.json"
    with open(schedule, "w") as handle:
        json.dump(batches, handle)
    answered = subprocess.run(
        [HARNESS, program, schedule, "--final"],
        capture_output=True,
        text=True,
        check=True,
    )
    # A demand rel's TABLE keeps every row it ever held, transients included;
    # adds minus dels over the tick lines is what a live host is handed.
    standing = {}
    for line in answered.stdout.splitlines():
        row = json.loads(line)
        deltas = row.get("deltas")
        if not isinstance(deltas, dict) or rel not in deltas:
            continue
        for gone in deltas[rel].get("del", []):
            standing.pop(gone[1], None)
        for fresh in deltas[rel].get("add", []):
            standing[fresh[1]] = fresh
    return list(standing.values())


# `due` requires `api_token`, so the token has to be answered before the first
# minute bucket or nothing polls at all.
def token_response(demand):
    _identity, witness, var_name, _bucket = demand
    return {
        "rel": "__host_response_env__var",
        "sign": "add",
        "row": [witness, 0, var_name, "scripted-token"],
    }


def response(demand, status, etag, remaining, body):
    _identity, witness, url, headers, prev_etag, bucket = demand
    # A whole-number header value is a JSON NUMBER, matching http.rs header_value.
    served = {
        "etag": etag,
        "x-ratelimit-remaining": remaining,
        "x-ratelimit-reset": 1000000,
    }
    return {
        "rel": "__host_response_http__get",
        "sign": "add",
        "row": [
            witness,
            0,
            url,
            headers,
            prev_etag,
            bucket,
            status,
            json.dumps(served, separators=(",", ":")),
            body,
            len(body) if status == 200 else 0,
        ],
    }


# A json null on any of gql_pull's captured fields answers zero rows, so
# `mergeable` stays a string, never GitHub's real null-on-merge.
def pr_node(number, state, updated_at, merged_at=None, merge_oid=None):
    return {
        "number": number,
        "title": "probe: close me",
        "state": state,
        "isDraft": False,
        "body": "probe, close me",
        "headRefName": "probe/pr-transition",
        "headRefOid": "deadbeef00",
        "baseRefName": "main",
        "mergeable": "MERGEABLE",
        "additions": 1,
        "deletions": 0,
        "changedFiles": 1,
        "createdAt": "2026-08-23T00:00:00Z",
        "updatedAt": updated_at,
        "mergedAt": merged_at,
        "closedAt": merged_at,
        "databaseId": 9001,
        "id": "PR_kwID_probe",
        "author": {"login": "hafley66"},
        "mergeCommit": {"oid": merge_oid} if merge_oid else None,
        "reviews": {"nodes": []},
        "comments": {"nodes": []},
        "reviewRequests": {"nodes": []},
        "labels": {"nodes": []},
        "commits": {"nodes": [{"commit": {"statusCheckRollup": None}}]},
    }


def graphql_response(demand, status, remaining, body):
    _identity, witness, url, headers, request_body, bucket = demand
    served = {"x-ratelimit-remaining": remaining, "x-ratelimit-reset": 1000000}
    payload = json.dumps(body, separators=(",", ":"))
    return {
        "rel": "__host_response_http__post",
        "sign": "add",
        "row": [
            witness,
            0,
            url,
            headers,
            request_body,
            bucket,
            status,
            json.dumps(served, separators=(",", ":")),
            payload,
            len(payload) if status == 200 else 0,
        ],
    }


def main():
    program = sys.argv[1]
    batches = [
        [
            clock(3600, 0, 0),
            {
                "rel": "__host_response_toml__json",
                "sign": "add",
                "row": [
                    "witness|toml__json|config_path:text=ghcache.toml|bucket:int=0",
                    0,
                    "ghcache.toml",
                    0,
                    CONFIG,
                ],
            },
        ]
    ]
    for demand in demands(program, batches, "__host_demand_env__var"):
        if demand[2] == "GITHUB_TOKEN":
            batches.append([token_response(demand)])
    # A 60s period is ONE minute bucket, so three consecutive buckets are three
    # polls: the first a 200, the next two conditional 304s moving zero bytes.
    # Bucket 3 adds a fresh (non-304) events answer so `dirty_repo` re-fires
    # `pr_due` once PR #1's first sweep already flipped `pr_ever_synced`.
    served = {}
    served_pr = {}
    pr_transitions = 0
    for ordinal, bucket in enumerate([0, 1, 2, 3]):
        batches.append([clock(60, ordinal, bucket)])
        # A demand rel keeps every row it ever held; a live host answers only
        # what THIS bucket asked, so the generator filters the same way.
        fresh = [
            row
            for row in demands(program, batches)
            if row[1] not in served and row[5] == bucket
        ]
        for demand in fresh:
            served[demand[1]] = True
            if bucket == 3:
                status, etag, remaining, body = (200, "etag-1", 4996, EVENTS_2)
            else:
                status, etag, remaining, body = (
                    (200, "etag-0", 4999, EVENTS)
                    if bucket == 0
                    else (304, "etag-0", 4999 - bucket, "null")
                )
            batches.append([response(demand, status, etag, remaining, body)])

        fresh_pr = [
            row
            for row in demands(program, batches, "__host_demand_http__post")
            if row[1] not in served_pr and row[5] == bucket
        ]
        for demand in fresh_pr:
            served_pr[demand[1]] = True
            if pr_transitions == 0:
                gql_body = {
                    "data": {
                        "rateLimit": {
                            "cost": 1,
                            "remaining": 4998,
                            "resetAt": "2026-08-23T01:00:00Z",
                        },
                        "repo_1": {
                            "pullRequests": {
                                "nodes": [pr_node(1, "OPEN", "2026-08-23T00:00:00Z")]
                            }
                        },
                        "repo_1_recent": {"pullRequests": {"nodes": []}},
                    }
                }
            else:
                gql_body = {
                    "data": {
                        "rateLimit": {
                            "cost": 1,
                            "remaining": 4996,
                            "resetAt": "2026-08-23T01:00:00Z",
                        },
                        "repo_1": {"pullRequests": {"nodes": []}},
                        "repo_1_recent": {
                            "pullRequests": {
                                "nodes": [
                                    pr_node(
                                        1,
                                        "MERGED",
                                        "2026-08-23T00:10:00Z",
                                        merged_at="2026-08-23T00:10:00Z",
                                        merge_oid="mergedsha001",
                                    )
                                ]
                            }
                        },
                    }
                }
            pr_transitions += 1
            batches.append([graphql_response(demand, 200, 4998, gql_body)])
    with open(f"{HERE}/ghcache.schedule.json", "w") as handle:
        json.dump(batches, handle, indent=2)
        handle.write("\n")
    print(f"batches={len(batches)} polls={len(served)} pr_batches={len(served_pr)}")


if __name__ == "__main__":
    main()
