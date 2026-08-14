export interface MetricDoc {
  session: string;
  snapshot: unknown;
}

export interface MetricSample {
  session: string;
  patch: unknown;
}
