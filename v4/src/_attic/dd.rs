// _attic/dd.rs — quarantined Differential Dataflow Store impl.
//
// Not in mod tree. Will not compile until reattached. Preserved as a
// reference for u32-keyed interning + timely worker shape. Author:
// "i'm done playing with it, storage is mem or sql, dont try to fight
// the greats" — 20260504.
//
// Reactivation cost (rough):
//   - re-add `differential-dataflow = "0.12"` and `timely = "0.12"` to
//     v4/Cargo.toml.
//   - re-attach via `mod _attic { pub mod dd; }` in lib.rs.
//   - re-export DdStore at the §3 Store impls boundary.
//   - port any new Store trait method changes since quarantine.
//
// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
// ░  § 3b  DdStore — differential-dataflow Store impl                  ░
// ▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░▓░
//
// Single-worker timely runtime, single fact "matches", N copies of the
// GroupCount rule pre-built at construction. Async store API marshals
// commands across a std::sync::mpsc channel into the worker thread.
// Lab scope: prove DD's incremental delta cost vs MemStore's full
// rederive. Generalize fact names / rule kinds after the proof point.

pub struct DdStore {
    cmd_tx:        std::sync::mpsc::SyncSender<DdCmd>,
    handle:        Mutex<Option<std::thread::JoinHandle<()>>>,
    tele:          OnceLock<Telemetry>,
    rule_outputs:  std::sync::RwLock<HashMap<String, broadcast::Sender<Diff>>>,
    /// Per-rule (row -> signed multiplicity) accumulator, updated from
    /// inspect_batch on the worker thread. Read from `snapshot()` to
    /// verify output parity against MemStore.
    rule_state:    Arc<Mutex<HashMap<String, HashMap<DdRow, isize>>>>,
    /// Shared with caller. Used to convert Cursor ↔ DdRow (id-keyed).
    interner:      Arc<Interner>,
}

/// DD-side row representation: 4-byte ids on each side, 8 bytes per
/// term vs ~32 bytes for (Arc<str>, Arc<str>). Repeated values
/// (paths, term names, pattern labels) collapse to one entry in the
/// interner regardless of how many rows reference them.
type DdRow = Vec<(u32, u32)>;

fn cursor_to_dd(c: &Cursor, interner: &Interner) -> DdRow {
    c.terms.iter()
        .map(|(n, v)| (interner.intern_id(n), interner.intern_id(v)))
        .collect()
}
fn dd_to_cursor(r: &DdRow, interner: &Interner) -> Cursor {
    let mut c = Cursor::default();
    for (n_id, v_id) in r {
        let n = interner.lookup(*n_id);
        let v = interner.lookup(*v_id);
        c.set_arc(&n, v);
    }
    c
}

enum DdCmd {
    Insert { fact: String, rows: Vec<DdRow>, gen: Gen },
    Commit { gen: Gen, ack: tokio::sync::oneshot::Sender<DdAck> },
    Stop,
}

#[derive(Default, Debug)]
pub struct DdAck { pub derived: u64, pub advance_ns: u64, pub step_ns: u64 }

impl DdStore {
    /// Build a DdStore with `rules` pre-attached over fact `fact_name`.
    /// Each rule is GroupCount(src=fact_name, key, min, count_term).
    pub fn new(fact_name: String, rules: Vec<(String, RuleBody)>, interner: Arc<Interner>) -> Arc<Self> {
        use differential_dataflow::input::Input;
        use differential_dataflow::operators::Reduce;

        let (cmd_tx, cmd_rx) = std::sync::mpsc::sync_channel::<DdCmd>(1024);
        let rule_outputs: HashMap<String, broadcast::Sender<Diff>> = rules.iter()
            .map(|(n, _)| (n.clone(), broadcast::channel(1024).0))
            .collect();
        let outputs_for_worker = rule_outputs.clone();
        let fact_for_worker = fact_name.clone();
        let rules_for_worker = rules.clone();
        let interner_for_worker = interner.clone();
        let rule_state: Arc<Mutex<HashMap<String, HashMap<DdRow, isize>>>> =
            Arc::new(Mutex::new(rules.iter().map(|(n, _)| (n.clone(), HashMap::new())).collect()));
        let rule_state_for_worker = rule_state.clone();

        // timely::execute_directly requires its worker closure to be
        // Send + Sync. Receiver isn't Sync, so wrap it. Single-threaded
        // worker contention is impossible (one worker, one thread) so
        // the Mutex is uncontended.
        let cmd_rx = Arc::new(Mutex::new(cmd_rx));
        let handle = std::thread::spawn(move || {
            let cmd_rx = cmd_rx.clone();
            timely::execute_directly(move |worker| {
                let cmd_rx = cmd_rx.lock().unwrap();
                use timely::dataflow::operators::probe::Handle as ProbeHandle;
                let derived_counter = Arc::new(AtomicU64::new(0));
                let mut probes: Vec<ProbeHandle<Gen>> = Vec::new();
                // Internal monotonic time. The Hooks.gen value is per-
                // trial and may not change between commits in a single
                // trial, which would let advance_to silently no-op.
                // We bump on every Commit, and route each Insert to the
                // current time. The caller's gen is preserved on Diff
                // for downstream subscribers.
                let mut dd_time: Gen = 0;
                let mut input = worker.dataflow::<Gen, _, _>(|scope| {
                    let (input, facts) = scope.new_collection::<DdRow, isize>();
                    for (rule_name, body) in &rules_for_worker {
                        let RuleBody::GroupCount { key, min, count_term, .. } = body else { continue };
                        let key_name_id    = interner_for_worker.intern_id(key);
                        let count_term_id  = interner_for_worker.intern_id(count_term);
                        let min = *min;
                        let out_tx = outputs_for_worker.get(rule_name).unwrap().clone();
                        let derived = derived_counter.clone();
                        let rule_state_inner = rule_state_for_worker.clone();
                        let rule_name_owned = rule_name.clone();
                        let interner_for_reduce  = interner_for_worker.clone();
                        let interner_for_inspect = interner_for_worker.clone();
                        let stream = facts
                            .map(move |r: DdRow| {
                                let k_id = r.iter().find(|(n, _)| *n == key_name_id)
                                    .map(|(_, v)| *v).unwrap_or(u32::MAX);
                                (k_id, r)
                            })
                            .reduce(move |k_id, vs, out| {
                                let n = vs.iter().map(|(_, m)| *m).sum::<isize>();
                                if n >= min as isize {
                                    let n_str_id = interner_for_reduce.intern_id(&n.to_string());
                                    let row: DdRow = vec![
                                        (key_name_id, *k_id),
                                        (count_term_id, n_str_id),
                                    ];
                                    out.push((row, 1));
                                }
                            })
                            .inspect_batch(move |t, batch| {
                                let mut state = rule_state_inner.lock().unwrap();
                                let entry = state.entry(rule_name_owned.clone()).or_default();
                                for ((_k, row), _t, sign) in batch {
                                    derived.fetch_add(1, Ordering::Relaxed);
                                    *entry.entry(row.clone()).or_insert(0) += *sign as isize;
                                    let _ = out_tx.send(Diff {
                                        row: dd_to_cursor(row, &interner_for_inspect),
                                        gen: *t,
                                        sign: *sign as i8,
                                    });
                                }
                            });
                        probes.push(stream.probe());
                    }
                    input
                });

                loop {
                    match cmd_rx.recv() {
                        Ok(DdCmd::Insert { fact, rows, gen: _ }) => {
                            if fact != fact_for_worker { continue; }
                            input.advance_to(dd_time);
                            for r in rows { input.insert(r); }
                        }
                        Ok(DdCmd::Commit { gen: _, ack }) => {
                            let t_adv = Instant::now();
                            dd_time += 1;
                            input.advance_to(dd_time);
                            input.flush();
                            let advance_ns = t_adv.elapsed().as_nanos() as u64;
                            let t_step = Instant::now();
                            let prev = derived_counter.load(Ordering::Relaxed);
                            while probes.iter().any(|p| p.less_than(input.time())) {
                                worker.step();
                            }
                            let step_ns = t_step.elapsed().as_nanos() as u64;
                            let derived = derived_counter.load(Ordering::Relaxed) - prev;
                            let _ = ack.send(DdAck { derived, advance_ns, step_ns });
                        }
                        Ok(DdCmd::Stop) => break,
                        Err(_) => break,
                    }
                }
            });
        });

        Arc::new(Self {
            cmd_tx,
            handle: Mutex::new(Some(handle)),
            tele: OnceLock::new(),
            rule_outputs: std::sync::RwLock::new(rule_outputs),
            rule_state,
            interner,
        })
    }
    pub fn attach_tele(&self, t: Telemetry) { let _ = self.tele.set(t); }
    fn span(&self, name: &'static str, n_in: Option<u64>) -> Option<SpanOpen> {
        self.tele.get().map(|t| t.start(name, n_in))
    }
}

#[async_trait::async_trait]
impl Store for DdStore {
    async fn insert(&self, fact: &str, row: Cursor, gen: Gen) {
        self.insert_many(fact, vec![row], gen).await
    }
    async fn insert_many(&self, fact: &str, rows: Vec<Cursor>, gen: Gen) {
        if rows.is_empty() { return; }
        let n = rows.len() as u64;
        let mut sp = self.span("v4::Dd::ins", Some(n));
        if let Some(s) = sp.as_mut() {
            let bytes: u64 = rows.iter().map(cursor_bytes).sum();
            s.set_bytes(bytes);
        }
        let dd_rows: Vec<DdRow> = rows.iter().map(|c| cursor_to_dd(c, &self.interner)).collect();
        let _ = self.cmd_tx.send(DdCmd::Insert { fact: fact.to_string(), rows: dd_rows, gen });
        if let Some(s) = sp { s.close(Some(n)); }
    }
    async fn commit(&self, gen: Gen) {
        let sp = self.span("v4::Dd::commit", None);
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let _ = self.cmd_tx.send(DdCmd::Commit { gen, ack: ack_tx });
        let ack = ack_rx.await.unwrap_or_default();
        // Synthesize child spans from worker-side accumulators so the
        // summary table breaks DD::commit into advance + step phases.
        if let Some(t) = self.tele.get() {
            push_synthetic_span(t, "v4::Dd::advance", ack.advance_ns, None);
            push_synthetic_span(t, "v4::Dd::step",    ack.step_ns,    Some(ack.derived));
        }
        if let Some(s) = sp { s.close(Some(ack.derived)); }
    }
    async fn remove(&self, _fact: &str, _row: Cursor, _gen: Gen) {}
    async fn forget_by(&self, _fact: &str, _key: &str, _v: &str, _gen: Gen) {}
    fn define_rule(&self, _name: &str, _body: RuleBody) {
        // No-op: DdStore takes rules at construction.
    }
    fn select(&self, name: &str) -> BoxStream<'static, Diff> {
        let tx = self.rule_outputs.write().unwrap()
            .entry(name.to_string())
            .or_insert_with(|| broadcast::channel(1024).0).clone();
        let rx = tx.subscribe();
        Box::pin(async_stream::stream! {
            let mut rx = rx;
            while let Ok(diff) = rx.recv().await { yield diff; }
        })
    }
    async fn snapshot(&self, name: &str) -> Vec<Cursor> {
        let state = self.rule_state.lock().unwrap();
        let Some(rule) = state.get(name) else { return vec![] };
        rule.iter()
            .filter(|(_, &mult)| mult > 0)
            .map(|(row, _)| dd_to_cursor(row, &self.interner))
            .collect()
    }
    async fn read_in(
        &self,
        _fact: &str,
        _key_col: &str,
        _key_values: Vec<String>,
        _project: Vec<String>,
    ) -> Vec<Cursor> {
        // Parked. DdStore is no longer on the active read path.
        vec![]
    }
}

impl Drop for DdStore {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(DdCmd::Stop);
        if let Some(h) = self.handle.lock().unwrap().take() { let _ = h.join(); }
    }
}
