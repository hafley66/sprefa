#!/usr/bin/env python3
"""Rebuild ghcache.account.schedule.json against the emitted program.

FAIL-PRE-FIX, measured in ~/.agent/dl6.db 2026-08-24 over 1427 minute buckets
(23.8 h): `users/hafley66/events/orgs/hafley66` answered 404 1422 times, one per
poll cycle, authenticated, decrementing the REST pool, because hafley66 is a
USER account and that endpoint exists for orgs only. Nothing backed the endpoint
off: a 404 re-demanded the next bucket, forever. `orgs/hafley66/repos` added 24
more 404s a day on the same shape.

This schedule pins both legs on one owner:

  bucket 0  `user` 200 + `orgs/<owner>/repos` 404 + `repos/<owner>/ghost/events` 404
  bucket 1  `not_an_org` folded: `users/<owner>/events` is the events spelling
            and `users/<owner>/events/orgs/<owner>` is never demanded at all
  bucket 2  the ghost repo's third 404, miss_streak hits miss_threshold(3)
  bucket 3  endpoint_cooling holds it out of `due`; the user-events poll runs on
  bucket 4  still cooling, and the cool-off runs to bucket 2 + 60

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

OWNER = "hafley66"
GHOST = f"repos/{OWNER}/ghost/events"
USER_EVENTS = f"users/{OWNER}/events"
ORG_EVENTS = f"users/{OWNER}/events/orgs/{OWNER}"
ORG_REPOS = f"orgs/{OWNER}/repos?per_page=100"

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
    # An `[[org]]` row is what mints `org_owner`, and `org_owner` is what makes
    # the account-type split reachable at all.
    "org": [
        {
            "owner": OWNER,
            "fs_alias": OWNER,
            "sync_prs": 0,
            "sync_events": 1,
            "sync_notifications": 0,
            "checkout_on_sync": 0,
            "checkout_pr_branches": 0,
        }
    ],
    # A repo that 404s every bucket: the backoff's subject, kept off the
    # graphql plane with sync_prs 0 so the receipt is REST calls only.
    "repo": [
        {
            "owner": OWNER,
            "name": "ghost",
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

WHOAMI = json.dumps({"login": OWNER}, separators=(",", ":"))
NO_EVENTS = "[]"


def clock(every, ordinal, bucket):
    return {
        "rel": "__host_response_clock__tick",
        "sign": "add",
        "row": [f"witness|clock__tick|every:int={every}", ordinal, every, bucket],
    }


def demands(program, batches, rel="__host_demand_http__get"):
    schedule = "/tmp/ghcache-account-step.json"
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


def token_response(demand):
    _identity, witness, var_name, _bucket = demand
    return {
        "rel": "__host_response_env__var",
        "sign": "add",
        "row": [witness, 0, var_name, "scripted-token"],
    }


def response(demand, status, etag, remaining, body):
    _identity, witness, url, headers, prev_etag, bucket = demand
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


# A 404 carries no etag: writing one would move `poll_state_etag` and put an
# If-None-Match on a resource the account does not have.
def answer_for(url, bucket, remaining):
    if url == ORG_REPOS or url == GHOST:
        return (404, "", remaining, "null")
    if url == "user":
        return (200, f"etag-user-{bucket}", remaining, WHOAMI)
    return (200, f"etag-events-{bucket}", remaining, NO_EVENTS)


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

    served = {}
    remaining = 5000
    for ordinal, bucket in enumerate([0, 1, 2, 3, 4]):
        batches.append([clock(60, ordinal, bucket)])
        # ONE pass per bucket, as the main schedule does; bucket 0's `user` 200
        # and `orgs/<owner>/repos` 404 land in ONE batch so both folds land together.
        fresh = [
            row
            for row in demands(program, batches)
            if row[1] not in served and row[5] == bucket
        ]
        batch = []
        for demand in fresh:
            served[demand[1]] = True
            remaining -= 1
            status, etag, left, body = answer_for(demand[2], bucket, remaining)
            batch.append(response(demand, status, etag, left, body))
        if batch:
            batches.append(batch)

    with open(f"{HERE}/ghcache.account.schedule.json", "w") as handle:
        json.dump(batches, handle, indent=2)
        handle.write("\n")
    print(f"batches={len(batches)} polls={len(served)}")
    for url in (ORG_EVENTS, USER_EVENTS, GHOST, ORG_REPOS):
        hits = sum(1 for witness in served if f"url:text={url}|" in witness)
        print(f"  {url} demanded={hits}")


if __name__ == "__main__":
    main()
