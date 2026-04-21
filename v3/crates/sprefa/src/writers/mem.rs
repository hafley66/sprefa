//! In-memory Writer. Implements strings + rule tables + rows.
//! Everything else is `unimplemented!()` until the first op that needs it lands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;
use tracing::info_span;

use crate::types::{FileId, RefId, RowId, StringId};
use crate::writer::{
    EffectLogRow, ExtractionRow, FileEdit, ProvenanceRow,
    RefEntry, RuleTableSpec, RunVisit, ShellCall, ShellReply, ViolationRow,
    WResult, Writer,
};

pub struct MemWriter {
    inner: Mutex<Inner>,
}

struct Inner {
    strings:    Vec<Arc<str>>,
    string_ix:  HashMap<Arc<str>, StringId>,
    tables:     HashMap<Arc<str>, TableState>,
}

struct TableState {
    spec: RuleTableSpec,
    rows: Vec<ExtractionRow>,
}

impl MemWriter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                strings:   vec![],
                string_ix: HashMap::new(),
                tables:    HashMap::new(),
            }),
        }
    }

    pub fn snapshot_rows(&self) -> HashMap<Arc<str>, Vec<ExtractionRow>> {
        let g = self.inner.lock().unwrap();
        g.tables.iter().map(|(k, v)| (k.clone(), v.rows.clone())).collect()
    }

    pub fn snapshot_strings(&self) -> Vec<Arc<str>> {
        self.inner.lock().unwrap().strings.clone()
    }
}

fn qualified(spec: &RuleTableSpec) -> Arc<str> {
    Arc::from(format!("{}__{}", spec.namespace, spec.rule))
}

impl Writer for MemWriter {
    fn create_rule_table(&self, spec: &RuleTableSpec) -> WResult<()> {
        let _s = info_span!("writer.mem.create_rule_table", rule = %spec.rule).entered();
        let mut g = self.inner.lock().unwrap();
        g.tables.entry(qualified(spec))
            .or_insert_with(|| TableState { spec: spec.clone(), rows: vec![] });
        Ok(())
    }

    fn write_strings(&self, values: &[&str]) -> WResult<Vec<StringId>> {
        let _s = info_span!("writer.mem.write_strings", n = values.len()).entered();
        let mut g = self.inner.lock().unwrap();
        let mut out = Vec::with_capacity(values.len());
        for v in values {
            let key: Arc<str> = Arc::from(*v);
            let id = if let Some(id) = g.string_ix.get(&key) {
                *id
            } else {
                let id = StringId(g.strings.len() as u64);
                g.strings.push(key.clone());
                g.string_ix.insert(key, id);
                id
            };
            out.push(id);
        }
        Ok(out)
    }

    fn write_refs     (&self, _: &[RefEntry])                   -> WResult<Vec<RefId>> { unimplemented!() }
    fn write_repos    (&self, _: &[&str])                       -> WResult<()> { Ok(()) }
    fn write_revs     (&self, _: &[(&str, &str)])               -> WResult<()> { Ok(()) }
    fn write_rev_files(&self, _: &[(&str, &str, FileId)])       -> WResult<()> { Ok(()) }

    fn write_rows(&self, spec: &RuleTableSpec, rows: &[ExtractionRow])
        -> WResult<Vec<RowId>>
    {
        let _s = info_span!("writer.mem.write_rows", rule = %spec.rule, n = rows.len()).entered();
        let mut g = self.inner.lock().unwrap();
        let qname = qualified(spec);
        let t = g.tables.entry(qname).or_insert_with(|| TableState {
            spec: spec.clone(),
            rows: vec![],
        });
        let start = t.rows.len() as u64;
        t.rows.extend_from_slice(rows);
        Ok((start..start + rows.len() as u64).map(RowId).collect())
    }

    fn write_provenance(&self, _: &[ProvenanceRow])                   -> WResult<()> { Ok(()) }
    fn write_run_visit (&self, _: &[RunVisit])                        -> WResult<()> { Ok(()) }
    fn write_violations(&self, _: &str, _: &[ViolationRow])           -> WResult<()> { Ok(()) }
    fn write_effect_log(&self, _: &[EffectLogRow])                    -> WResult<()> { Ok(()) }
    fn rewrite_files   (&self, _: &[FileEdit])                        -> WResult<()> { unimplemented!() }

    fn shell_batch(&self, _: &[ShellCall]) -> BoxStream<'static, Vec<ShellReply>>
    { unimplemented!() }

    fn flush(&self) -> BoxStream<'static, ()> {
        Box::pin(stream::iter(std::iter::once(())))
    }
}

/// `bytes` type is unused in this file; suppress unused_imports by referencing it here.
#[allow(dead_code)]
fn _keep_bytes_import(_: Bytes) {}
