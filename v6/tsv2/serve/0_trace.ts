/** Served-engine tracing on the shared diagnostics-channel spine.
 * It is off unless `DL_PERF_LOG` is set and emits one aggregated line per
 * tick. Effects and bind firings are folded into the next tick line.
 */

import diagnostics_channel from "node:diagnostics_channel";

import pino, { type Logger } from "pino";

export const SERVE_CHANNEL_NAMES = {
  tick: "sprefa:tick",
  effect: "sprefa:effect",
  bind: "sprefa:bind",
  watch: "sprefa:watch",
} as const;

const tick_channel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.tick);
const effect_channel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.effect);
const bind_channel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.bind);
const watch_channel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.watch);
// Published by the RUNTIME (runtime/trace.ts), which knows which rule a
// statement came from and nothing about where the record should go.
const rule_channel = diagnostics_channel.channel(RUNTIME_CHANNEL_NAMES.rule);

import { RUNTIME_CHANNEL_NAMES } from "../runtime/trace.ts";
import type {
  IServeBindEvent as BindEvent,
  IServeEffectEvent as EffectEvent,
  IServeRuleEvent as RuleEvent,
  IServeTickEvent as TickEvent,
  IServeTickLine,
  IServeTrace,
  IServeWatchEvent as WatchEvent,
} from "../runtime/types.ts";

let logger: Logger | null = null;
let pending_rules: RuleEvent[] = [];
let pending_effects: EffectEvent[] = [];
let pending_binds: BindEvent[] = [];
let pending_watches: WatchEvent[] = [];
let installed = false;

function install(log_path: string): void {
  // `base: null` drops pid/hostname and `timestamp: false` the clock. pino
  // still prepends its own numeric `level`, and it stays: suppressing it with
  // `formatters.level` makes pino emit `{,"tick":...`, malformed JSON, since
  // the formatter's object is spliced in as a prefix chunk. Sink decoration is
  // not the contract -- registry.pl's trace_event fields are, and
  // tests/traceGolden.test.ts grades the line's PROJECTION onto them.
  logger = pino({ base: null, timestamp: false }, pino.destination({ dest: log_path, sync: true }));
  rule_channel.subscribe((message) => {
    pending_rules.push(message as RuleEvent);
  });
  effect_channel.subscribe((message) => {
    pending_effects.push(message as EffectEvent);
  });
  bind_channel.subscribe((message) => {
    pending_binds.push(message as BindEvent);
  });
  watch_channel.subscribe((message) => {
    pending_watches.push(message as WatchEvent);
  });
  tick_channel.subscribe((message) => {
    const event = message as TickEvent;
    const line: IServeTickLine = {
      ...event,
      actor: "tsv2.serve",
      seam: "tick",
      rules: pending_rules,
      effects: pending_effects,
      binds: pending_binds,
      watches: pending_watches,
    };
    pending_rules = [];
    pending_effects = [];
    pending_binds = [];
    pending_watches = [];
    logger?.info(line);
  });
  installed = true;
}

export const ServeTrace: IServeTrace = {
  tick(tick, rels, rows, statements, wall_ms): void {
    if (!tick_channel.hasSubscribers) return;
    tick_channel.publish({ tick, rels, rows, statements, wall_ms: wall_ms } satisfies TickEvent);
  },

  effect(host, witness_digest, outcome, rows, wall_ms, failure): void {
    if (!effect_channel.hasSubscribers) return;
    const error = failure === undefined ? undefined : failure instanceof Error ? failure.message : String(failure);
    effect_channel.publish({
      host,
      witness_digest: witness_digest,
      outcome,
      rows,
      wall_ms: wall_ms,
      error,
    } satisfies EffectEvent);
  },

  bind(rel, period, bucket): void {
    if (!bind_channel.hasSubscribers) return;
    bind_channel.publish({ rel, period, bucket } satisfies BindEvent);
  },

  watch(rel, glob, added, removed): void {
    if (!watch_channel.hasSubscribers) return;
    watch_channel.publish({ rel, glob, added, removed } satisfies WatchEvent);
  },

  install_from_env(): void {
    const log_path = process.env.DL_PERF_LOG;
    if (installed || log_path === undefined || log_path.length === 0) return;
    install(log_path);
  },
};

ServeTrace.install_from_env();
