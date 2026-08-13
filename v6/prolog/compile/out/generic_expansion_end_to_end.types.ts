export interface AuthorAudit {
  id: number;
  tag: string;
}

export interface Bucket {
  id: number;
}

export interface Metric {
  id: number;
  value: number | null;
}

export interface MetricCopy {
  id: number;
  value: number | null;
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
  title: string | null;
}

export interface TicketCopy {
  id: number;
  title: string | null;
}
