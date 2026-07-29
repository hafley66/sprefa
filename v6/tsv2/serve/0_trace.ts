/**
 * 0_trace.ts — the served engine's self-diagnosis, on the EXISTING P0 spine.
 *
 * Class-34 law: no parallel telemetry pipeline. `node:diagnostics_channel` is
 * a PROCESS-GLOBAL registry keyed by name, so publishing on `sprefa:effect` and
 * `sprefa:bind` here is publishing on the very same channels v6/dl's
 * `0_trace.ts` publishes on -- same spine, one more producer, not a second
 * pipeline. `sprefa:tick` is this arc's one new channel name (the served tick
 * loop has no counterpart in v6/dl, whose tick spans ride `sprefa:sql`).
 *
 * Off by default: DL_PERF_LOG unset means nothing subscribes, and every publish
 * site checks `hasSubscribers` first.
 *
 * Aggregation: exactly ONE line per tick, never one per statement or per
 * effect (the N+1 law applies to this module's own output too). Effects and
 * bind firings that land between two ticks are folded into the next tick's
 * line. pino is the emit layer, the same buy v6/dl's spine made
 * (plans/2026-07-27-perf-tracing-buy-verdict.md).
 */

import diagnostics_channel from "node:diagnostics_channel";

import pino, { type Logger } from "pino";

export const SERVE_CHANNEL_NAMES = {
  tick: "sprefa:tick",
  effect: "sprefa:effect",
  bind: "sprefa:bind",
  watch: "sprefa:watch",
} as const;

const tickChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.tick);
const effectChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.effect);
const bindChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.bind);
const watchChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.watch);

import type {
  IServeBindEvent as BindEvent,
  IServeEffectEvent as EffectEvent,
  IServeTickEvent as TickEvent,
  IServeTickLine,
  IServeTrace,
  IServeWatchEvent as WatchEvent,
} from "../runtime/types.ts";

let logger: Logger | null = null;
let pendingEffects: EffectEvent[] = [];
let pendingBinds: BindEvent[] = [];
let pendingWatches: WatchEvent[] = [];
let installed = false;

function install(logPath: string): void {
  logger = pino({ base: null, timestamp: false }, pino.destination({ dest: logPath, sync: true }));
  effectChannel.subscribe((message) => {
    pendingEffects.push(message as EffectEvent);
  });
  bindChannel.subscribe((message) => {
    pendingBinds.push(message as BindEvent);
  });
  watchChannel.subscribe((message) => {
    pendingWatches.push(message as WatchEvent);
  });
  tickChannel.subscribe((message) => {
    const event = message as TickEvent;
    const line: IServeTickLine = { ...event, effects: pendingEffects, binds: pendingBinds, watches: pendingWatches };
    pendingEffects = [];
    pendingBinds = [];
    pendingWatches = [];
    logger?.info(line);
  });
  installed = true;
}

export const ServeTrace: IServeTrace = {
  tick(tick, rels, rows, statements, ms): void {
    if (!tickChannel.hasSubscribers) return;
    tickChannel.publish({ tick, rels, rows, statements, ms } satisfies TickEvent);
  },

  effect(host, witnessDigest, outcome, rows, ms, failure): void {
    if (!effectChannel.hasSubscribers) return;
    const error = failure === undefined ? undefined : failure instanceof Error ? failure.message : String(failure);
    effectChannel.publish({ host, witnessDigest, outcome, rows, ms, error } satisfies EffectEvent);
  },

  bind(rel, period, bucket): void {
    if (!bindChannel.hasSubscribers) return;
    bindChannel.publish({ rel, period, bucket } satisfies BindEvent);
  },

  watch(rel, glob, added, removed): void {
    if (!watchChannel.hasSubscribers) return;
    watchChannel.publish({ rel, glob, added, removed } satisfies WatchEvent);
  },

  installFromEnv(): void {
    const logPath = process.env.DL_PERF_LOG;
    if (installed || logPath === undefined || logPath.length === 0) return;
    install(logPath);
  },
};

ServeTrace.installFromEnv();
