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
            "sync_prs": 0,
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


def demands(program, batches):
    schedule = "/tmp/ghcache-schedule-step.json"
    with open(schedule, "w") as handle:
        json.dump(batches, handle)
    answered = subprocess.run(
        [HARNESS, program, schedule, "--final"],
        capture_output=True,
        text=True,
        check=True,
    )
    for line in answered.stdout.splitlines():
        row = json.loads(line)
        if row.get("rel") == "__host_demand_http__get":
            return row["rows"]
    return []


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
    # A 60s period is ONE minute bucket, so three consecutive buckets are three
    # polls: the first a 200, the next two conditional 304s moving zero bytes.
    served = {}
    for ordinal, bucket in enumerate([0, 1, 2]):
        batches.append([clock(60, ordinal, bucket)])
        fresh = [row for row in demands(program, batches) if row[1] not in served]
        for demand in fresh:
            served[demand[1]] = True
            status, etag, remaining, body = (
                (200, "etag-0", 4999, EVENTS) if bucket == 0 else (304, "etag-0", 4999 - bucket, "null")
            )
            batches.append([response(demand, status, etag, remaining, body)])
    with open(f"{HERE}/ghcache.schedule.json", "w") as handle:
        json.dump(batches, handle, indent=2)
        handle.write("\n")
    print(f"batches={len(batches)} polls={len(served)}")


if __name__ == "__main__":
    main()
