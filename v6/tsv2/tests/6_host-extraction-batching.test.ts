import assert from "node:assert/strict";
import { test } from "node:test";

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
  IHostEffectDone,
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

type Executor = (
  host: string,
  commandLine: string,
  env: Record<string, string>,
) => Observable<string>;

type Scenario = {
  readonly effects: readonly IHostEffectDone[];
  readonly submitted: readonly IArrivalBatch[];
  readonly hostRuns: number;
  readonly extractorRuns: number;
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
    demandRel: `__host_demand_${name}`,
    responseRel: `__host_response_${name}`,
  };
}

function programFor(plans: readonly IHostPlan[]): IServedProgram {
  const relColumns: Record<string, readonly string[]> = {};
  for (const current of plans) {
    relColumns[current.demandRel] = [
      "identity_digest",
      "witness_digest",
      ...current.inputs.map((input) => input.name),
    ];
    relColumns[current.responseRel] = [
      "witness_digest",
      "ordinal",
      "path",
      ...current.outputs.map((output) => output.name),
    ];
  }
  return {
    ddl: [],
    statements: [],
    relColumns,
    relKinds: {},
    relKeys: {},
    relRefColumns: {},
    relStructColumns: {},
    boot: [],
    finalSelect: {},
    hostPlans: plans,
    bindPlans: [],
    queryPlans: [],
    unsupportedExecution: [],
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
        rel: current.demandRel,
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

async function runScenario(
  plans: readonly IHostPlan[],
  ticks: readonly ITickOutcome[],
  expectedEffects: number,
  options: {
    readonly bootRows?: Readonly<Record<string, readonly IRow[]>>;
    readonly answered?: readonly {
      readonly host: string;
      readonly witnessDigest: string;
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
        answered.witnessDigest,
        "done",
        1,
      ),
    );
  }
  const tickSource = new Subject<ITickOutcome>();
  const submitted: IArrivalBatch[] = [];
  let hostRuns = 0;
  let extractorRuns = 0;

  const executors = new Map<string, Executor>([
    [
      "shell",
      (host) => {
        hostRuns += 1;
        if (host === "resolve_at") extractorRuns += 1;
        return of("");
      },
    ],
    [
      "sprefa_extract",
      () => {
        hostRuns += 1;
        extractorRuns += 1;
        return of(EXTRACT_STDOUT);
      },
    ],
  ]);
  const engine = {
    program: programFor(plans),
    ticks$: tickSource,
    rows: (rel: string) => of(options.bootRows?.[rel] ?? []),
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
    const effectsPromise = firstValueFrom(
      new HostRunner(engine, seam, plans, executors).effects$.pipe(
        take(expectedEffects),
        toArray(),
      ),
    );
    await new Promise<void>((resolve) => setImmediate(resolve));
    for (const outcome of ticks) tickSource.next(outcome);
    const effects = await effectsPromise;
    tickSource.complete();
    return { effects, submitted, hostRuns, extractorRuns };
  } finally {
    seam.db.close();
  }
}

test("callgraph, diagnostics, and flow obey one extractor process per path and digest", async () => {
  const files = [
    ["a.ts", "a1"],
    ["b.ts", "b1"],
    ["c.ts", "c1"],
  ] as const;
  const enumerate = plan("enumerate_at", "shell", columns("found"), "enumerate {path}");
  const callPlans = [
    plan("call_node", "sprefa_extract", columns("record", "family", "kind", "name")),
    plan("call_ref", "sprefa_extract", columns("record", "family", "callee")),
  ];
  const callgraph = await runScenario(
    [enumerate, ...callPlans],
    [
      tick(1, [enumerate], [["repo", "rev"]]),
      tick(2, callPlans, files),
    ],
    1 + callPlans.length * files.length,
  );
  assert.deepEqual(
    {
      hostRuns: callgraph.hostRuns,
      extractorRuns: callgraph.extractorRuns,
      responseBatches: callgraph.submitted.length,
    },
    { hostRuns: 1 + files.length, extractorRuns: files.length, responseBatches: files.length },
  );

  const diag = await runScenario(
    callPlans,
    [tick(1, callPlans, files)],
    callPlans.length * files.length,
  );
  assert.deepEqual(
    {
      hostRuns: diag.hostRuns,
      extractorRuns: diag.extractorRuns,
      responseBatches: diag.submitted.length,
    },
    { hostRuns: files.length, extractorRuns: files.length, responseBatches: files.length },
  );

  const resolve = plan("resolve_at", "shell", columns("edge"), "resolve {path}");
  const flowPlans = [
    plan("df_node_at", "sprefa_extract", columns("record", "family", "span", "kind")),
    plan("df_edge_at", "sprefa_extract", columns("record", "family", "kind", "from", "to")),
    plan("df_param_at", "sprefa_extract", columns("record", "family", "span", "pos")),
    plan("df_arg_at", "sprefa_extract", columns("record", "family", "call", "pos", "arg")),
    plan("call_node_at", "sprefa_extract", columns("record", "family", "span", "kind", "name")),
    plan("type_node_at", "sprefa_extract", columns("record", "family", "span", "kind", "name")),
    plan("sig_at", "sprefa_extract", columns("record", "family", "owner_start", "owner_end", "slot", "pos", "ty")),
  ];
  const flow = await runScenario(
    [enumerate, resolve, ...flowPlans],
    [
      tick(1, [enumerate, resolve], [["repo", "rev"]]),
      tick(2, flowPlans, files),
    ],
    2 + flowPlans.length * files.length,
  );
  assert.deepEqual(
    {
      hostRuns: flow.hostRuns,
      extractorRuns: flow.extractorRuns,
      responseBatches: flow.submitted.length,
    },
    {
      hostRuns: 2 + files.length,
      extractorRuns: 1 + files.length,
      responseBatches: files.length,
    },
  );
});

test("extractor batching is frontier-local, digest-separated, and ignores demand retractions", async () => {
  const projections = [
    plan("call_node", "sprefa_extract", columns("record", "family", "kind", "name")),
    plan("call_ref", "sprefa_extract", columns("record", "family", "callee")),
  ];
  const oldInput = [["a.ts", "old"]] as const;
  const nextInput = [["a.ts", "next"]] as const;
  const result = await runScenario(
    projections,
    [
      tick(1, projections, oldInput),
      tick(2, projections, oldInput, "del"),
      tick(3, projections, oldInput),
      tick(4, projections, nextInput),
    ],
    projections.length * 2,
  );

  assert.equal(result.hostRuns, 2);
  assert.equal(result.extractorRuns, 2);
  assert.equal(result.submitted.length, 2);
  assert.deepEqual(
    result.submitted.map((batch) => [...new Set(batch.map((arrival) => arrival.rel))].sort()),
    [
      ["__host_response_call_node", "__host_response_call_ref"],
      ["__host_response_call_node", "__host_response_call_ref"],
    ],
  );
  assert.deepEqual(
    result.effects.map(({ host, witnessDigest, outcome }) => ({ host, witnessDigest, outcome })),
    [
      { host: "call_node", witnessDigest: "witness|call_node|a.ts|old", outcome: "done" },
      { host: "call_ref", witnessDigest: "witness|call_ref|a.ts|old", outcome: "done" },
      { host: "call_node", witnessDigest: "witness|call_node|a.ts|next", outcome: "done" },
      { host: "call_ref", witnessDigest: "witness|call_ref|a.ts|next", outcome: "done" },
    ],
  );
});

test("generic shell demands remain one process per witness", async () => {
  const shellPlans = [
    plan("left_shell", "shell", [], "same {path}"),
    plan("right_shell", "shell", [], "same {path}"),
  ];
  const result = await runScenario(
    shellPlans,
    [tick(1, shellPlans, [["a.ts", "same"]])],
    2,
  );
  assert.deepEqual(
    { hostRuns: result.hostRuns, extractorRuns: result.extractorRuns },
    { hostRuns: 2, extractorRuns: 0 },
  );
});

test("a cached projection is omitted while its unanswered sibling still runs", async () => {
  const projections = [
    plan("call_node", "sprefa_extract", columns("record", "family", "kind", "name")),
    plan("call_ref", "sprefa_extract", columns("record", "family", "callee")),
  ];
  const nodeWitness = "witness|call_node|a.ts|same";
  const refWitness = "witness|call_ref|a.ts|same";
  const result = await runScenario(projections, [], 1, {
    answered: [{ host: "call_node", witnessDigest: nodeWitness }],
    bootRows: {
      __host_demand_call_node: [
        ["identity|call_node|a.ts", nodeWitness, "a.ts", "same"],
      ],
      __host_demand_call_ref: [
        ["identity|call_ref|a.ts", refWitness, "a.ts", "same"],
      ],
    },
  });
  assert.deepEqual(
    {
      hostRuns: result.hostRuns,
      extractorRuns: result.extractorRuns,
      effects: result.effects.map(({ host, witnessDigest, outcome }) => ({
        host,
        witnessDigest,
        outcome,
      })),
      responseRels: result.submitted.flatMap((batch) =>
        batch.map((arrival) => arrival.rel),
      ),
    },
    {
      hostRuns: 1,
      extractorRuns: 1,
      effects: [
        {
          host: "call_ref",
          witnessDigest: refWitness,
          outcome: "done",
        },
      ],
      responseRels: ["__host_response_call_ref"],
    },
  );
});
