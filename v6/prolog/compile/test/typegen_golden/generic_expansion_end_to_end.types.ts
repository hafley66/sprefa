export type Option<T> = { tag: 'none' } | { tag: 'some'; value: T };

export interface AuthorAudit {
  id: number;
  tag: string;
}

export interface Bucket {
  id: number;
}

export interface Metric {
  id: number;
  value: Option<number>;
}

export interface MetricCopy {
  id: number;
  value: Option<number>;
}

export interface Person {
  id: number;
  name: string;
}

export interface PriorityHigh {
  id: number;
}

export interface PriorityLow {
  id: number;
}

export interface PriorityTag {
  id: number;
  tag: string;
}

export interface Review {
  id: number;
}

export interface SeenBucket {
  id: number;
  list_id: number;
}

export interface SeenReview {
  id: number;
  name: string;
}

export interface SeenTicket {
  id: number;
  tag: string;
}

export interface Ticket {
  id: number;
  title: Option<string>;
}

export interface TicketCopy {
  id: number;
  title: Option<string>;
}
