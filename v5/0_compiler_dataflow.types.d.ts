/**
 * Top-down map of the DL6 application, compiler, emitted artifacts, and runtime.
 *
 * Read every FlowN left to right:
 *
 *     Noun, Verb, Noun, Verb, Noun
 *
 * Nouns become graph nodes. Verbs become directed graph edges. Nested namespaces
 * retain the ownership and border context needed when the graph is expanded.
 */

export namespace $0_Application {
  export type Entrypoints = {
    compile: Compile.Flow
    serve: Serve.Flow
  }

  export namespace Compile {
    export type Flow = $Flow.Flow6<
      $Request.Compile,
      $Compiler.LoadModules,
      $Source.LoadedModules,
      $Compiler.ParseModules,
      $Syntax.ParsedModules,
      $Compiler.TypeModules,
      $Semantics.TypedProgram,
      $Compiler.LowerProgram,
      $Lowering.LoweredProgram,
      $Compiler.SelectEmitter,
      $Emit.SelectedProgram,
      $Compiler.EmitArtifacts,
      $Artifact.PendingSet
    >
  }

  export namespace Serve {
    export type Flow = $Flow.Flow3<
      $Artifact.CommittedSet,
      $Runtime.LoadProgram,
      $Runtime.LoadedProgram,
      $Runtime.StartProgram,
      $Runtime.RunningProgram,
      $Runtime.HandleRequests,
      $Runtime.ResponseStream
    >
  }
}

export namespace $Flow {
  export type Through<Input, Verb, Output> = {
    input: Input
    verb: Verb
    output: Output
  }

  export type Flow2<A, AB, B, BC, C> = readonly [
    Through<A, AB, B>,
    Through<B, BC, C>,
  ]

  export type Flow3<A, AB, B, BC, C, CD, D> = readonly [
    Through<A, AB, B>,
    Through<B, BC, C>,
    Through<C, CD, D>,
  ]

  export type Flow4<A, AB, B, BC, C, CD, D, DE, E> = readonly [
    Through<A, AB, B>,
    Through<B, BC, C>,
    Through<C, CD, D>,
    Through<D, DE, E>,
  ]

  export type Flow6<
    A,
    AB,
    B,
    BC,
    C,
    CD,
    D,
    DE,
    E,
    EF,
    F,
    FG,
    G,
  > = readonly [
    Through<A, AB, B>,
    Through<B, BC, C>,
    Through<C, CD, D>,
    Through<D, DE, E>,
    Through<E, EF, F>,
    Through<F, FG, G>,
  ]
}

export namespace $Request {
  export type Compile = {
    entryModules: readonly $Border.FileSystem.Path[]
    configuration: $Configuration.Compiler
    target: $Target.Selection
  }
}

export namespace $Compiler {
  export type LoadModules = {
    operation: "load_modules"
    crosses: $Border.FileSystem.Read
  }

  export type ParseModules = {
    operation: "parse_modules"
    consumes: $Source.LoadedModules
    produces: $Syntax.ParsedModules
  }

  export type TypeModules = {
    operation: "type_modules"
    consumes: $Syntax.ParsedModules
    produces: $Semantics.TypedProgram
  }

  export type LowerProgram = {
    operation: "lower_program"
    consumes: $Semantics.TypedProgram
    produces: $Lowering.LoweredProgram
  }

  export type SelectEmitter = {
    operation: "select_emitter"
    selectedBy: $Configuration.Compiler["target"]
  }

  export type EmitArtifacts = {
    operation: "emit_artifacts"
    crosses:
      | $Border.FileSystem.Write
      | $Border.Sqlite.Transaction
      | $Border.RustCompiler.Invocation
  }
}

export namespace $Source {
  export type ModuleId = string

  export type LoadedModules = {
    roots: readonly ModuleId[]
    modules: ReadonlyMap<ModuleId, LoadedModule>
  }

  export type LoadedModule = {
    id: ModuleId
    path: $Border.FileSystem.Path
    text: string
    imports: readonly ModuleId[]
  }
}

export namespace $Syntax {
  export type ParsedModules = {
    roots: readonly $Source.ModuleId[]
    modules: ReadonlyMap<$Source.ModuleId, ParsedModule>
    diagnostics: readonly Diagnostic[]
  }

  export type ParsedModule = {
    id: $Source.ModuleId
    declarations: readonly Declaration[]
  }

  export type Declaration = {
    kind: "rel" | "interface" | "implementation" | "rule" | "module"
    name: string
  }

  export type Diagnostic = {
    module: $Source.ModuleId
    message: string
    severity: "error" | "warning"
  }
}

export namespace $Semantics {
  export type DeclarationId = string
  export type ParameterId = string
  export type ApplicationId = string

  export type TypedProgram = {
    declarations: ReadonlyMap<DeclarationId, Declaration>
    parameters: ReadonlyMap<ParameterId, Parameter>
    members: readonly Member[]
    constraints: readonly Constraint[]
    applications: ReadonlyMap<ApplicationId, Application>
    implementations: readonly Implementation[]
  }

  export type Declaration = {
    id: DeclarationId
    module: $Source.ModuleId
    name: string
    kind: "rel" | "interface"
    storage: "stored" | "derived" | "contract"
  }

  export type Parameter = {
    id: ParameterId
    owner: DeclarationId
    ordinal: number
    name: string
  }

  export type Member = {
    owner: DeclarationId
    ordinal: number
    name: string
    type: Type
  }

  export type Type =
    | { kind: "primitive"; name: "bool" | "int" | "float" | "string" | "json" }
    | { kind: "parameter"; parameter: ParameterId }
    | { kind: "application"; application: ApplicationId }

  export type Constraint = {
    subject: ParameterId
    interface: DeclarationId
  }

  export type Application = {
    id: ApplicationId
    constructor: DeclarationId
    arguments: readonly Type[]
  }

  export type Implementation = {
    subject: Type
    interface: ApplicationId
  }
}

export namespace $Lowering {
  export type LoweredProgram = {
    modules: readonly Module[]
    relations: readonly Relation[]
    rules: readonly Rule[]
    runtimeWiring: RuntimeWiring
  }

  export type Module = {
    id: $Source.ModuleId
    dependencies: readonly $Source.ModuleId[]
  }

  export type Relation = {
    declaration: $Semantics.DeclarationId
    columns: readonly $Semantics.Member[]
  }

  export type Rule = {
    reads: readonly $Semantics.DeclarationId[]
    writes: readonly $Semantics.DeclarationId[]
  }

  export type RuntimeWiring = {
    modules: readonly $Source.ModuleId[]
    events: readonly EventBinding[]
  }

  export type EventBinding = {
    event: string
    relation: $Semantics.DeclarationId
  }
}

export namespace $Emit {
  export type SelectedProgram =
    | { target: $Target.Tsv2; program: $Lowering.LoweredProgram }
    | { target: $Target.RustIr; program: $Lowering.LoweredProgram }
    | { target: $Target.RustDyn; program: $Lowering.LoweredProgram }
    | { target: $Target.RustStatic; program: $Lowering.LoweredProgram }

  export namespace Tsv2 {
    export type Flow = $Flow.Flow2<
      SelectedProgram,
      EmitTypeScript,
      $Artifact.TypeScriptSources,
      EmitSqliteSchema,
      $Artifact.PendingSet
    >

    export type EmitTypeScript = { operation: "emit_typescript" }
    export type EmitSqliteSchema = { operation: "emit_sqlite_schema" }
  }

  export namespace RustIr {
    export type Flow = $Flow.Flow3<
      SelectedProgram,
      EmitRust,
      $Artifact.RustSources,
      EmitSqliteSchema,
      $Artifact.RustAndSqlite,
      EmitIrRequests,
      $Artifact.PendingSet
    >

    export type EmitRust = { operation: "emit_rust" }
    export type EmitSqliteSchema = { operation: "emit_sqlite_schema" }
    export type EmitIrRequests = { operation: "emit_ir_requests" }
  }

  export namespace RustDyn {
    export type Flow = $Flow.Flow3<
      SelectedProgram,
      EmitRustModules,
      $Artifact.RustSources,
      EmitSqliteSchema,
      $Artifact.RustAndSqlite,
      CompileDynamicLibraries,
      $Artifact.PendingSet
    >

    export type EmitRustModules = { operation: "emit_rust_modules" }
    export type EmitSqliteSchema = { operation: "emit_sqlite_schema" }
    export type CompileDynamicLibraries = {
      operation: "compile_dynamic_libraries"
      crosses: $Border.RustCompiler.Invocation
    }
  }

  export namespace RustStatic {
    export type Flow = $Flow.Flow2<
      SelectedProgram,
      EmitRustWorkspace,
      $Artifact.RustSources,
      CompileStaticBinary,
      $Artifact.PendingSet
    >

    export type EmitRustWorkspace = { operation: "emit_rust_workspace" }
    export type CompileStaticBinary = {
      operation: "compile_static_binary"
      crosses: $Border.RustCompiler.Invocation
    }
  }
}

export namespace $Target {
  export type Selection = "tsv2" | "rust_ir" | "rust_dyn" | "rust_static"

  export type Tsv2 = {
    name: "tsv2"
    language: "typescript"
    storage: "sqlite"
    execution: "typescript"
  }

  export type RustIr = {
    name: "rust_ir"
    language: "rust"
    storage: "sqlite"
    execution: "ir_request"
  }

  export type RustDyn = {
    name: "rust_dyn"
    language: "rust"
    storage: "sqlite"
    execution: "dynamic_rust"
  }

  export type RustStatic = {
    name: "rust_static"
    language: "rust"
    storage: "rust"
    execution: "static_rust"
  }
}

export namespace $Artifact {
  export type PendingSet = {
    files: readonly $Border.FileSystem.PendingWrite[]
    database: readonly $Border.Sqlite.PendingOperation[]
    compiler: readonly $Border.RustCompiler.PendingInvocation[]
  }

  export type CommittedSet = {
    files: readonly $Border.FileSystem.CommittedFile[]
    databases: readonly $Border.Sqlite.Database[]
    binaries: readonly $Border.Process.Executable[]
  }

  export type TypeScriptSources = {
    files: readonly $Border.FileSystem.PendingWrite[]
  }

  export type RustSources = {
    files: readonly $Border.FileSystem.PendingWrite[]
  }

  export type RustAndSqlite = {
    rust: RustSources
    sqlite: readonly $Border.Sqlite.PendingOperation[]
  }
}

export namespace $Runtime {
  export type LoadProgram = {
    operation: "load_program"
    crosses: $Border.Process.Load
  }

  export type LoadedProgram = {
    artifacts: $Artifact.CommittedSet
    modules: readonly LoadedModule[]
  }

  export type LoadedModule = {
    name: string
    execution: $Target.Tsv2["execution"]
      | $Target.RustIr["execution"]
      | $Target.RustDyn["execution"]
      | $Target.RustStatic["execution"]
  }

  export type StartProgram = {
    operation: "start_program"
    crosses: $Border.Process.Spawn
  }

  export type RunningProgram = {
    modules: readonly LoadedModule[]
    registrations: readonly Registration[]
  }

  export type Registration = {
    protocol: "http" | "lsp" | "mcp" | "event"
    route: string
  }

  export type HandleRequests = {
    operation: "handle_requests"
    crosses: $Border.Protocol.RequestResponse
  }

  export type ResponseStream = {
    responses: AsyncIterable<$Border.Protocol.Response>
  }
}

export namespace $Configuration {
  export type Compiler = {
    target: $Target.Selection
    outputDirectory: $Border.FileSystem.Path
    moduleMode: "single_binary" | "binary_per_module"
  }
}

export namespace $Border {
  export namespace FileSystem {
    export type Path = string
    export type Read = { domain: "filesystem"; capability: "read" }
    export type Write = { domain: "filesystem"; capability: "write" }
    export type PendingWrite = { path: Path; contents: string | Uint8Array }
    export type CommittedFile = { path: Path; bytes: number }
  }

  export namespace Sqlite {
    export type Transaction = { domain: "sqlite"; capability: "transaction" }
    export type PendingOperation = { database: FileSystem.Path; statement: string }
    export type Database = { path: FileSystem.Path }
  }

  export namespace RustCompiler {
    export type Invocation = { domain: "rust_compiler"; capability: "compile" }
    export type PendingInvocation = {
      manifest: FileSystem.Path
      artifactKind: "binary" | "dynamic_library"
    }
  }

  export namespace Process {
    export type Load = { domain: "process"; capability: "load" }
    export type Spawn = { domain: "process"; capability: "spawn" }
    export type Executable = { path: FileSystem.Path }
  }

  export namespace Protocol {
    export type RequestResponse = {
      domain: "protocol"
      capability: "request_response"
    }

    export type Response = {
      protocol: "http" | "lsp" | "mcp" | "event"
      payload: Uint8Array
    }
  }
}
