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
