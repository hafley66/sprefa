import assert from "node:assert/strict";
import { test } from "vitest";

import {
  Observable,
  Subject,
  firstValueFrom,
  of,
  take,
  toArray,
} from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import type {
  IArrivalBatch,
  IHostAdapterRow,
  IHostEffectDone,
  IProcessAdapter,
  IHostPlan,
  ILiveEngine,
  IRow,
  IRowValue,
  IServedProgram,
  ITickOutcome,
} from "../runtime/types.ts";
import { HostRunner, WitnessCache } from "../serve/1_hosts.ts";

const EXTRACT_TEMPLATE =
  '"$DL_EXTRACT_BIN" --family cst,type,call,df {path}';
const EXTRACT_STDOUT = [
  JSON.stringify({
    record: "node",
    family: "call",
    span: { start: 0, end: 3 },
    kind: "function",
    name: "main",
  }),
  JSON.stringify({
    record: "site",
    family: "call",
    callee: "work",
  }),
  JSON.stringify({
    record: "param",
    family: "df",
    span: { start: 4, end: 5 },
    pos: 0,
  }),
  JSON.stringify({
    record: "sig",
    family: "type",
    owner_start: 0,
    owner_end: 3,
    slot: "return",
    pos: 0,
    ty: "number",
  }),
].join("\n");

type Scenario = {
  readonly effects: readonly IHostEffectDone[];
  readonly submitted: readonly IArrivalBatch[];
  readonly host_runs: number;
  readonly extractor_runs: number;
};

function columns(...names: string[]) {
  return names.map((name) => ({ name, type: name === "pos" ? "int" : "text" }));
}

function plan(
  name: string,
  execution: "shell" | "sprefa_extract",
  outputs: IHostPlan["outputs"],
  template = EXTRACT_TEMPLATE,
): IHostPlan {
  return {
    name,
    execution,
    template,
    inputs: columns("path", "digest"),
    outputs,
    demand_rel: `__host_demand_${name}`,
    response_rel: `__host_response_${name}`,
  };
}

function program_for(plans: readonly IHostPlan[]): IServedProgram {
  const rel_columns: Record<string, readonly string[]> = {};
  for (const current of plans) {
    rel_columns[current.demand_rel] = [
      "identity_digest",
      "witness_digest",
      ...current.inputs.map((input) => input.name),
    ];
    rel_columns[current.response_rel] = [
      "witness_digest",
      "ordinal",
      "path",
      ...current.outputs.map((output) => output.name),
    ];
  }
  return {
    ddl: [],
    statements: [],
    rel_columns,
    rel_kinds: {},
    rel_keys: {},
    rel_ref_columns: {},
    rel_struct_columns: {},
    boot: [],
    final_select: {},
    host_plans: plans,
    bind_plans: [],
    query_plans: [],
    unsupported_execution: [],
  } as unknown as IServedProgram;
}

function tick(
  number: number,
  plans: readonly IHostPlan[],
  inputs: readonly (readonly [path: string, digest: string])[],
  sign: "add" | "del" = "add",
): ITickOutcome {
  return {
    tick: number,
    line: {
      tick: number,
      deltas: {},
    },
    deltas: {
      rels: plans.map((current) => ({
        rel: current.demand_rel,
        add:
          sign === "add"
            ? inputs.map(([path, digest]) => [
                `identity|${current.name}|${path}`,
                `witness|${current.name}|${path}|${digest}`,
                path,
                digest,
              ])
            : [],
        del:
          sign === "del"
            ? inputs.map(([path, digest]) => [
                `identity|${current.name}|${path}`,
                `witness|${current.name}|${path}|${digest}`,
                path,
                digest,
              ])
            : [],
      })),
      collapse: [],
    },
  } as unknown as ITickOutcome;
}

async function run_scenario(
  plans: readonly IHostPlan[],
  ticks: readonly ITickOutcome[],
  expected_effects: number,
  options: {
    readonly boot_rows?: Readonly<Record<string, readonly IRow[]>>;
    readonly adapter_rows?: readonly IHostAdapterRow[];
    readonly answered?: readonly {
      readonly host: string;
      readonly witness_digest: string;
    }[];
  } = {},
): Promise<Scenario> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, WitnessCache.ddl()));
  for (const answered of options.answered ?? []) {
    await firstValueFrom(
      WitnessCache.settle(
        seam,
        answered.host,
        answered.witness_digest,
        "done",
        1,
      ),
    );
  }
  const tick_source = new Subject<ITickOutcome>();
  const submitted: IArrivalBatch[] = [];
  let host_runs = 0;
  let extractor_runs = 0;

  const executors = new Map<string, IProcessAdapter>([
    [
      "shell",
      {
        name: "shell",
        applicative: false,
        command(demand) {
          host_runs += 1;
          if (demand.plan.name === "resolve_at") extractor_runs += 1;
          return { argv: [], env: {}, stdin: "" };
        },
        decode() { return []; },
      },
    ],
    [
      "sprefa_extract",
      {
        name: "sprefa_extract",
        applicative: true,
        command() {
          host_runs += 1;
          extractor_runs += 1;
          return { argv: [], env: {}, stdin: EXTRACT_STDOUT };
        },
        decode(_stdout, plan) { return [plan.outputs.map(() => "")]; },
      },
    ],
  ]);
  const engine = {
    program: program_for(plans),
    ticks$: tick_source,
    rows: (rel: string) => of(options.boot_rows?.[rel] ?? []),
    submit: (arrivals: IArrivalBatch) => {
      submitted.push(arrivals);
      return of({
        tick: 1000 + submitted.length,
        line: { tick: 1000 + submitted.length, deltas: {} },
        deltas: { rels: [], collapse: [] },
      } as unknown as ITickOutcome);
    },
  } as ILiveEngine;

  try {
    const effects_promise = firstValueFrom(
      new HostRunner(engine, seam, plans, executors, options.adapter_rows).effects$.pipe(
        take(expected_effects),
        toArray(),
      ),
    );
    await new Promise<void>((resolve) => setImmediate(resolve));
    for (const outcome of ticks) tick_source.next(outcome);
    const effects = await effects_promise;
    tick_source.complete();
    return { effects, submitted, host_runs, extractor_runs };
  } finally {
    seam.db.close();
  }
}

function extract_rows(plans: readonly IHostPlan[]): readonly IHostAdapterRow[] {
  return plans.map((current) => ({
    adapter: "sprefa_extract",
    demand_rel: current.demand_rel,
    response_rel: current.response_rel,
  }));
}

test("callgraph, diagnostics, and flow obey one extractor process per path and digest", async () => {
  const files = [
    ["a.ts", "a1"],
    ["b.ts", "b1"],
    ["c.ts", "c1"],
  ] as const;
  const files_at = plan("files_at", "shell", columns("found"), "files_at {path}");
  const call_plans = [
    plan("call_node", "shell", columns("record", "family", "kind", "name")),
    plan("call_ref", "shell", columns("record", "family", "callee")),
  ];
  const callgraph = await run_scenario(
    [files_at, ...call_plans],
    [
      tick(1, [files_at], [["repo", "rev"]]),
      tick(2, call_plans, files),
    ],
    1 + call_plans.length * files.length,
    { adapter_rows: extract_rows(call_plans) },
  );
  assert.deepEqual(
    {
      host_runs: callgraph.host_runs,
      extractor_runs: callgraph.extractor_runs,
      response_batches: callgraph.submitted.length,
    },
    { host_runs: 1 + files.length, extractor_runs: files.length, response_batches: files.length },
  );

  const diag = await run_scenario(
    call_plans,
    [tick(1, call_plans, files)],
    call_plans.length * files.length,
    { adapter_rows: extract_rows(call_plans) },
  );
  assert.deepEqual(
    {
      host_runs: diag.host_runs,
      extractor_runs: diag.extractor_runs,
      response_batches: diag.submitted.length,
    },
    { host_runs: files.length, extractor_runs: files.length, response_batches: files.length },
  );

  const resolve = plan("resolve_at", "shell", columns("edge"), "resolve {path}");
  const flow_plans = [
    plan("df_node_at", "shell", columns("record", "family", "span", "kind")),
    plan("df_edge_at", "shell", columns("record", "family", "kind", "from", "to")),
    plan("df_param_at", "shell", columns("record", "family", "span", "pos")),
    plan("df_arg_at", "shell", columns("record", "family", "call", "pos", "arg")),
    plan("call_node_at", "shell", columns("record", "family", "span", "kind", "name")),
    plan("type_node_at", "shell", columns("record", "family", "span", "kind", "name")),
    plan("sig_at", "shell", columns("record", "family", "owner_start", "owner_end", "slot", "pos", "ty")),
  ];
  const flow = await run_scenario(
    [files_at, resolve, ...flow_plans],
    [
      tick(1, [files_at, resolve], [["repo", "rev"]]),
      tick(2, flow_plans, files),
    ],
    2 + flow_plans.length * files.length,
    { adapter_rows: extract_rows(flow_plans) },
  );
  assert.deepEqual(
    {
      host_runs: flow.host_runs,
      extractor_runs: flow.extractor_runs,
      response_batches: flow.submitted.length,
    },
    {
      host_runs: 2 + files.length,
      extractor_runs: 1 + files.length,
      response_batches: files.length,
    },
  );
});

test("extractor batching is frontier-local, digest-separated, and ignores demand retractions", async () => {
  const projections = [
    plan("call_node", "shell", columns("record", "family", "kind", "name")),
    plan("call_ref", "shell", columns("record", "family", "callee")),
  ];
  const old_input = [["a.ts", "old"]] as const;
  const next_input = [["a.ts", "next"]] as const;
  const result = await run_scenario(
    projections,
    [
      tick(1, projections, old_input),
      tick(2, projections, old_input, "del"),
      tick(3, projections, old_input),
      tick(4, projections, next_input),
    ],
    projections.length * 2,
    { adapter_rows: extract_rows(projections) },
  );

  assert.equal(result.host_runs, 2);
  assert.equal(result.extractor_runs, 2);
  assert.equal(result.submitted.length, 2);
  assert.deepEqual(
    result.submitted.map((batch) => [...new Set(batch.map((arrival) => arrival.rel))].sort()),
    [
      ["__host_response_call_node", "__host_response_call_ref"],
      ["__host_response_call_node", "__host_response_call_ref"],
    ],
  );
  assert.deepEqual(
    result.effects.map(({ host, witness_digest, outcome }) => ({ host, witness_digest, outcome })),
    [
      { host: "call_node", witness_digest: "witness|call_node|a.ts|old", outcome: "done" },
      { host: "call_ref", witness_digest: "witness|call_ref|a.ts|old", outcome: "done" },
      { host: "call_node", witness_digest: "witness|call_node|a.ts|next", outcome: "done" },
      { host: "call_ref", witness_digest: "witness|call_ref|a.ts|next", outcome: "done" },
    ],
  );
});

test("generic shell demands remain one process per witness", async () => {
  const shell_plans = [
    plan("left_shell", "shell", [], "same {path}"),
    plan("right_shell", "shell", [], "same {path}"),
  ];
  const result = await run_scenario(
    shell_plans,
    [tick(1, shell_plans, [["a.ts", "same"]])],
    2,
  );
  assert.deepEqual(
    { host_runs: result.host_runs, extractor_runs: result.extractor_runs },
    { host_runs: 2, extractor_runs: 0 },
  );
});

test("a cached projection is omitted while its unanswered sibling still runs", async () => {
  const projections = [
    plan("call_node", "shell", columns("record", "family", "kind", "name")),
    plan("call_ref", "shell", columns("record", "family", "callee")),
  ];
  const node_witness = "witness|call_node|a.ts|same";
  const ref_witness = "witness|call_ref|a.ts|same";
  const result = await run_scenario(projections, [], 1, {
    answered: [{ host: "call_node", witness_digest: node_witness }],
    adapter_rows: extract_rows(projections),
    boot_rows: {
      __host_demand_call_node: [
        ["identity|call_node|a.ts", node_witness, "a.ts", "same"],
      ],
      __host_demand_call_ref: [
        ["identity|call_ref|a.ts", ref_witness, "a.ts", "same"],
      ],
    },
  });
  assert.deepEqual(
    {
      host_runs: result.host_runs,
      extractor_runs: result.extractor_runs,
      effects: result.effects.map(({ host, witness_digest, outcome }) => ({
        host,
        witness_digest,
        outcome,
      })),
      response_rels: result.submitted.flatMap((batch) =>
        batch.map((arrival) => arrival.rel),
      ),
    },
    {
      host_runs: 1,
      extractor_runs: 1,
      effects: [
        {
          host: "call_ref",
          witness_digest: ref_witness,
          outcome: "done",
        },
      ],
      response_rels: ["__host_response_call_ref"],
    },
  );
});
