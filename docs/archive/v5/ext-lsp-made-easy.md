# Making an LSP server easy, not a labyrinth — three Rust approaches

Read of three cloned sources, scored against sprefa v5's hard constraint: a
**synchronous tick engine** with **no async parked-wake queue**. Every sprefa
action is "insert a `request` fact, run ONE inline synchronous tick, read
`answer` facts, retract". The LSP transport must hand us raw messages and let us
process each one inline on a single thread. It must not own an async runtime or
demand we return futures.

Sources cloned under `/Users/chrishafley/projects/ext/`:

| crate | author | shape |
|-------|--------|-------|
| `tower-lsp` | ebkalderon | async, tower `Service`, derive/trait-based |
| `async-lsp` | oxalica | modern tower middleware `Layer` stack |
| `rust-analyzer/lib/lsp-server` | rust-analyzer | minimal sync crate, crossbeam channels, no async runtime |

sprefa already ships on `lsp-server` today: `src/lsp.rs:11`
`use lsp_server::{Connection, Message, Notification};`. This doc confirms that
choice and sketches the next steps.

---

## 1. tower-lsp — the `LanguageServer` trait + `#[async_trait]`

**Core abstraction the user implements:** a trait with one `async fn` per LSP
method. You impl it on your own `Backend` struct.

- Trait declared at `src/lib.rs:116-118`:
  `#[async_trait] pub trait LanguageServer: Send + Sync + 'static`.
- Every method is `async`: `initialize` (`src/lib.rs:126`), `did_open`
  (`src/lib.rs:165`), `did_change` (`src/lib.rs:178`), `did_save`
  (`src/lib.rs:218`), `hover`, `completion`, `goto_definition` (`src/lib.rs:281`),
  etc. ~40 default-bodied async methods; you override the ones you serve.
- Wiring types: `LspService`, `Server`, `Client` (`examples/stdio.rs:4`).
  `Client` is the handle you call back through (`self.client.log_message(...).await`).

**Smallest hello-world** (`examples/stdio.rs`, 132 lines, the canonical minimum):

```rust
struct Backend { client: Client }

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> { /* caps */ }
    async fn shutdown(&self) -> Result<()> { Ok(()) }
    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> { /* ... */ }
}

#[tokio::main]
async fn main() {
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
```

Ceremony count: `#[tokio::main]`, `#[tower_lsp::async_trait]`, the `Client`
plumbing field, `async fn` on every handler, `.await` on every client call.
Mandatory ceremony to reach the first response is ~25 LOC, but all of it is
async-shaped.

**Threading / runtime model:** FORCES tokio. `main` is `#[tokio::main]`
(`examples/stdio.rs:118`). `Server::serve` is `pub async fn serve`
(`src/transport.rs:102`). The crate OWNS the dispatch loop; it decodes JSON-RPC,
routes to your trait method, and (`src/transport.rs:78-95`) spawns concurrent
request handling (default via `tokio::spawn`). You never see a `Message`. A sync
per-request handler has nowhere to live except inside an `async fn` body, where
it would block the executor.

**Three message shapes:**
- request→response: return a value from the `async fn` (e.g. `completion`
  returns `Result<Option<CompletionResponse>>`).
- notification (didChange): override `async fn did_change`
  (`src/lib.rs:178`), no return.
- long-running request / `$/progress`: you `tokio::spawn` and `.await`, then
  drive progress through `self.client`. The framework's concurrency
  (`src/transport.rs:78`) interleaves it with other requests. This is exactly
  the async parked-wake queue sprefa rejected.

---

## 2. async-lsp — `LspService` / `LanguageServer` + a tower `Layer` stack

**Core abstraction the user implements:** the same per-method idea, but each
method returns a `BoxFuture` instead of being `async fn`, and you compose
behavior by stacking tower middleware `Layer`s around a `Router`.

- `LspService: Service<AnyRequest>` (`src/lib.rs:170`) — built on `tower::Service`.
  `notify` (`src/lib.rs:179`) and `emit` (`src/lib.rs:190`) handle notifications
  and user events; notifications are "delivered in order and synchronously"
  (the comment, `src/lib.rs:173`).
- The convenience server trait `LanguageServer` (`examples/server_trait.rs:25`):
  `type Error`, `type NotifyResult`, and methods like
  `fn initialize(&mut self, ...) -> BoxFuture<'static, Result<...>>`
  (`examples/server_trait.rs:29-44`), `fn hover(...) -> BoxFuture<...>`
  (`examples/server_trait.rs:46`).
- `Router::from_language_server(...)` (`examples/server_trait.rs:85`) adapts your
  struct into the `Service`. `MainLoop` (`src/lib.rs:467`) drives it on an
  `mpsc::UnboundedReceiver<MainLoopEvent>` (`src/lib.rs:469`).

**Smallest hello-world** (`examples/server_trait.rs`, 142 lines). The `main`
is the tell:

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (server, _) = async_lsp::MainLoop::new_server(|client| {
        ServiceBuilder::new()
            .layer(TracingLayer::default())
            .layer(LifecycleLayer::default())
            .layer(CatchUnwindLayer::default())
            .layer(ConcurrencyLayer::default())
            .layer(ClientProcessMonitorLayer::new(client.clone()))
            .service(ServerState::new_router(client))
    });
    let (stdin, stdout) = (PipeStdin::lock_tokio()?, PipeStdout::lock_tokio()?);
    server.run_buffered(stdin, stdout).await.unwrap();
}
```

Ceremony count: highest of the three. `#[tokio::main]`, a five-`Layer`
`ServiceBuilder`, `Box::pin(async move { ... })` wrapping every handler body
(`examples/server_trait.rs:34`, `:49`), a `Router`, and async pipe stdio. The
middleware stack is the selling point and the cost.

**Threading / runtime model:** FORCES tokio (`#[tokio::main(flavor = "current_thread")]`,
`examples/server_trait.rs:97`). `MainLoop::run_buffered` is async
(`examples/server_trait.rs:141`). The crate owns dispatch via `MainLoop`
(`src/lib.rs:467`, `dispatch_event` at `:616`). Concurrency is a Layer
(`src/concurrency.rs:1-8`, `pub struct Concurrency<S>` at `:32`) that spawns a
task per request. A sync handler can return `std::future::ready(...)` to avoid
awaiting, but the runtime, the executor poll loop, and the `mpsc` event queue
are unavoidable.

**Three message shapes:**
- request→response: return a `BoxFuture` resolving to the result
  (`examples/server_trait.rs:46-64`).
- notification (didChange): `LspService::notify` (`src/lib.rs:179`) or a router
  notification handler, returning `ControlFlow`.
- long-running / `$/progress`: spawn inside the future; `ConcurrencyLayer`
  (`src/concurrency.rs`) caps in-flight requests and wires `$/cancelRequest`.
  Again, an async parked-wake queue by construction.

---

## 3. lsp-server — the `Connection` struct + raw `Message` match loop

**Core abstraction the user implements:** nothing. There is no trait. You get a
`Connection` (a pair of channels) and you write your own `for msg in
&connection.receiver { match msg { ... } }` loop.

- `src/lib.rs:1`: "A language server scaffold, exposing a **synchronous
  crossbeam-channel based API**."
- `src/lib.rs:30-34`: `/// Connection is just a pair of channels of LSP
  messages. pub struct Connection { pub sender: Sender<Message>, pub receiver:
  Receiver<Message> }`. Channels are `crossbeam_channel` (`src/lib.rs:21`).
- `Connection::stdio()` (`src/lib.rs:40`) returns `(Connection, IoThreads)`.
- `connection.initialize(caps)` (`src/lib.rs:280`) does the
  initialize/initialized handshake synchronously and returns the params.
- `connection.handle_shutdown(&req)` (`src/lib.rs:348`) acks shutdown and waits
  for `exit`, returning `true` when you should break the loop.
- `ReqQueue` (`src/req_queue.rs:7`) tracks pending incoming/outgoing requests
  for cancellation and `$/progress` — opt-in, you call `register`/`complete`
  (`src/req_queue.rs:39`, `:53`) only if you want it.

**Smallest hello-world** (`examples/minimal_lsp.rs`, 336 lines but most is
rustfmt + completion demo content). The structural minimum:

```rust
fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_thread) = Connection::stdio();
    let caps = ServerCapabilities { /* ... */ };
    let init_params = connection.initialize(serde_json::json!({ "capabilities": caps }))?;
    main_loop(connection, init_params)?;
    io_thread.join()?;
    Ok(())
}

fn main_loop(connection: Connection, _: serde_json::Value) -> Result<...> {
    for msg in &connection.receiver {                       // blocking recv
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? { break; }
                // ... match req.method, build Response, connection.sender.send(...)
            }
            Message::Notification(note) => { /* match note.method */ }
            Message::Response(_) => {}
        }
    }
    Ok(())
}
```

(`examples/minimal_lsp.rs:116-141` for `main`, `:147-173` for `main_loop`.)
Ceremony count: a `for`/`match`, plus tiny `send_ok`/`send_err` helpers
(`examples/minimal_lsp.rs:312-335`). No trait, no macro, no `async`, no `.await`.
The structural minimum to first response is ~20 plain-sync LOC.

**Threading / runtime model:** NO async runtime. `stdio_transport`
(`src/stdio.rs:13`) spawns exactly three plain OS threads with
`thread::Builder` — a reader (`src/stdio.rs:33`), a writer (`src/stdio.rs:16`),
and a dropper (`src/stdio.rs:28`) — bridged to your loop by bounded crossbeam
channels (`src/stdio.rs:14-15`). Those threads only do framed JSON-RPC byte
I/O. **You own the dispatch loop**; the crate hands you `Message`s on
`connection.receiver` and you process each one inline on your thread. A sync
per-request handler lives directly in the `match` arm — which is precisely where
a sprefa tick goes.

**Three message shapes:**
- request→response: `match req.method`, build a `Response`,
  `connection.sender.send(Message::Response(resp))` (`examples/minimal_lsp.rs:312-316`).
- notification (didChange): `match note.method`, e.g. `DidChangeTextDocument`
  (`examples/minimal_lsp.rs:191-198`), no reply.
- long-running / `$/progress`: nothing forced. Because YOU own the loop, the
  default is: run it inline, send the response, move to the next message. If you
  ever want background work you spawn your own `std::thread` and push results
  back via `connection.sender` (it is just a channel). `ReqQueue`
  (`src/req_queue.rs`) is there if you want cancellation, but it is optional and
  sync. There is no parked-wake queue unless you build one.

---

## Recommendation

| criterion (weighted for sprefa) | tower-lsp | async-lsp | lsp-server |
|---|---|---|---|
| no forced async runtime | 0 — `#[tokio::main]` required | 0 — `#[tokio::main]` required | **2 — three plain OS I/O threads only** |
| hands you raw `Message`s (you own dispatch) | 0 — owns loop, calls your trait | 0 — `MainLoop` owns loop | **2 — `for msg in &receiver`** |
| sync per-request handler fits naturally | 0 — must be inside `async fn` | 1 — `ready()` works but runtime stays | **2 — handler IS the match arm** |
| one inline tick per request, zero parked-wake | 0 — concurrency spawns tasks | 0 — `ConcurrencyLayer` spawns tasks | **2 — natural; bg work is opt-in** |
| ceremony to first response | 1 — ~25 async LOC + 2 macros | 0 — Layer stack + `BoxFuture` everywhere | **2 — ~20 plain-sync LOC** |
| middleware / batteries (cancel, progress) | 1 — built-in but async | **2 — richest Layer set** | 1 — `ReqQueue` opt-in, sync |
| already adopted by sprefa | — | — | **yes (`src/lsp.rs`)** |
| **total (sync-tick fit)** | **2** | **3** | **13** |

**Winner: `lsp-server`.** It is the only one of the three that does not impose
an async runtime, does not own the dispatch loop, and lets a synchronous handler
be the literal `match` arm. The other two are excellent if you want async
concurrency; both directly contradict the sprefa "one inline synchronous tick
per action" rule. async-lsp's middleware stack is the strongest feature set but
buys the exact async machinery sprefa rejected from v4.

### sprefa LSP loop on the winner (extends today's `src/lsp.rs`)

`lsp.rs` is 164 LOC and already runs this shape for didSave/didOpen only
(`src/lsp.rs:51-71`): cold tick, then per-notification `tick_paths` + publish.
Generalize the match arm to the "insert request fact → one tick → read answers →
retract" protocol so requests (hover, definition) join notifications:

```rust
for msg in &connection.receiver {
    match msg {
        Message::Request(req) => {
            if connection.handle_shutdown(&req)? { break; }
            let (rel, args) = request_to_fact(&req);          // e.g. ("hover_req", [uri,line,col])
            eng.assert_fact(rel, &args)?;                     // insert `request` fact
            eng.tick(&prog, false)?;                          // ONE inline synchronous tick
            let answers = eng.read_answers(&req.method)?;     // read `answer` facts
            eng.retract_fact(rel, &args)?;                    // retract the request
            connection.sender.send(answers_to_response(req.id, answers).into())?;
        }
        Message::Notification(note) => {
            if let Some(abs) = touched_path(&note) {          // didSave / didChange
                eng.tick_paths(&prog, &[abs.clone()], true)?; // the existing path
                publish(&connection, &eng, &root, Some(&abs))?;
            }
        }
        Message::Response(_) => {}
    }
}
```

No tokio, no `.await`, no spawned task. Each request is one tick on the single
loop thread, exactly the rejected-v4-async-free model. Long-running work, if ever
needed, is an explicit `std::thread` writing back through
`connection.sender` — opt-in, never the default.
