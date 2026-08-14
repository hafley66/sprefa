export interface ArtifactPlan {
  name: string;
  target: EmitterTarget;
  program: ProgramPlan;
  format: string;
  path: string;
  linkage: string;
  load_phase: string;
}

export interface BindPlan {
  name: string;
  source_kind: string;
  target_relation: RelationPlan;
  execution: string;
}

export interface DeploymentArtifact {
  deployment_name: string;
  ordinal: number;
  artifact: ArtifactPlan;
}

export interface DeploymentPlan {
  name: string;
  runtime: RuntimeBed;
  assembly_kind: string;
  binary_count: number;
  dynamic_loader: string | null;
}

export interface EmitterTarget {
  name: string;
  language: string;
  artifact_kind: string;
  execution_kind: string;
  assembly_kind: string;
}

export interface EventPlan {
  name: string;
  source: ModulePlan;
  target: ModulePlan;
  delivery: string;
  cardinality: string;
  lifetime: string;
}

export interface HostPlan {
  name: string;
  demand_relation: RelationPlan;
  response_relation: RelationPlan;
  execution: string;
}

export interface LoweringIntent {
  name: string;
  target: EmitterTarget;
  kind: string;
  input_kind: string;
  output_kind: string;
}

export interface ModulePlan {
  name: string;
  role: string;
  artifact_path: string;
  load_phase: string;
  entry: boolean;
}

export interface ProgramBind {
  program_name: string;
  ordinal: number;
  bind: BindPlan;
}

export interface ProgramHost {
  program_name: string;
  ordinal: number;
  host: HostPlan;
}

export interface ProgramPlan {
  name: string;
  target: EmitterTarget;
  entry_module: ModulePlan;
  subscription: SubscriptionPlan;
  intern_mode: string;
}

export interface ProgramQuery {
  program_name: string;
  ordinal: number;
  query: QueryPlan;
}

export interface ProgramRelation {
  program_name: string;
  ordinal: number;
  relation: RelationPlan;
}

export interface ProgramStatement {
  program_name: string;
  ordinal: number;
  statement: StatementPlan;
}

export interface QueryPlan {
  name: string;
  relation: RelationPlan;
  snapshot: string;
  transport_path: string;
}

export interface RelationPlan {
  name: string;
  arity: number;
  storage_kind: string;
  arrival_target: boolean;
  subscribed: boolean;
}

export interface RuntimeBed {
  name: string;
  target: EmitterTarget;
  storage: StoragePlan;
  server: ServerPlan;
}

export interface RuntimeEvent {
  runtime_name: string;
  ordinal: number;
  event: EventPlan;
}

export interface RuntimeModule {
  runtime_name: string;
  ordinal: number;
  module: ModulePlan;
}

export interface RustDynTarget {
  target: EmitterTarget;
  rust_runtime: string;
  sqlite_backend: string;
  generated_module_kind: string;
  linkage: string;
  dynamic_loader: string | null;
}

export interface RustIrTarget {
  target: EmitterTarget;
  rust_runtime: string;
  sqlite_backend: string;
  ir_format: string;
  request_boundary: string;
}

export interface ServerPlan {
  harness: string;
  transport: string;
  program_cardinality: string;
  process_lifetime: string;
}

export interface StatementPlan {
  name: string;
  phase: string;
  relation: RelationPlan;
  sql: string;
  params_shape: unknown;
}

export interface StoragePlan {
  backend: string;
  database_scope: string;
  transaction_owner: string;
  state_kind: string;
}

export interface SubscriptionPlan {
  root_relation: RelationPlan;
  activation: string;
  release: string;
  cone_kind: string;
}

export interface TargetIntent {
  target_name: string;
  ordinal: number;
  intent: LoweringIntent;
}

export interface Tsv2Target {
  target: EmitterTarget;
  typescript_runtime: string;
  sqlite_backend: string;
  module_format: string;
}
