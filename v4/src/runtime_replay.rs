use std::sync::Arc;

use effect_runtime::v2::{
    expand, EventBus, ExpandOpts, FactStore, Node, PipeInstance, QueueBackend,
};

use crate::mounted_query;
use crate::runtime_graph::RuntimeGraph;
use crate::store::SprfStore;
use crate::Cursor;

pub struct GraphReplayRunner {
    pub facts: Arc<dyn FactStore<Cursor>>,
    pub queue: Arc<dyn QueueBackend<Cursor>>,
    pub bus: Arc<EventBus>,
    pub sprf_store: Arc<SprfStore>,
    pub runtime_graph: Arc<RuntimeGraph>,
    pub instances: Vec<Arc<PipeInstance<Cursor>>>,
}

impl GraphReplayRunner {
    pub fn drain(&self) -> usize {
        let mut handled = 0;
        for _ in 0..8 {
            let jobs = self.runtime_graph.pending_jobs();
            if jobs.is_empty() {
                break;
            }
            for job in jobs {
                let Some(owner) = self.runtime_graph.owner_descriptor(job.owner_uri_id) else {
                    continue;
                };
                if self
                    .runtime_graph
                    .continuation_for_owner(job.owner_uri_id)
                    .is_some()
                {
                    if self.rerun_continuation_owner(job.owner_uri_id, job.generation) {
                        self.runtime_graph.mark_job_done(&job);
                        handled += 1;
                    }
                    continue;
                }
                let Some(mount_id) = owner.ast_uri.strip_prefix("sprf://ast/sql/") else {
                    continue;
                };
                let Some(snapshot) = mounted_query::load_mounted_sql_snapshot(
                    self.facts.as_ref(),
                    mount_id,
                    owner.input_key.as_ref(),
                ) else {
                    continue;
                };
                let batch: Vec<&Cursor> = snapshot.inputs.iter().collect();
                let Ok(nodes) = crate::sql::run_sql_batch(
                    self.facts.as_ref(),
                    snapshot.sql.as_str(),
                    &snapshot.dep_tables,
                    &batch,
                ) else {
                    continue;
                };
                let ctx = effect_runtime::v2::RenderCtx::new(0, 0, job.generation)
                    .with_bus(self.bus.clone())
                    .with_runtime(self.runtime_graph.clone());
                mounted_query::record_runtime_sql_mount(
                    &ctx,
                    snapshot.sql.as_str(),
                    &batch,
                    &snapshot.dep_tables,
                    &nodes,
                );
                let dirty_tables = mounted_query::dirty_tables_for_sql_outputs(
                    self.facts.as_ref(),
                    snapshot.sql.as_str(),
                    &batch,
                    &nodes,
                );
                mounted_query::record_sql_outputs(
                    &self.facts,
                    snapshot.sql.as_str(),
                    job.generation,
                    &batch,
                    &snapshot.dep_tables,
                    &nodes,
                );
                for table in dirty_tables {
                    let source = self
                        .runtime_graph
                        .declare_source(&format!("sprf://source/table/{table}"), job.generation);
                    self.runtime_graph.dispatch_dirty(&source, job.generation);
                }
                self.sprf_store.flush();
                self.facts.commit(job.generation, Some(&self.bus));
                let seeds = node_seeds(&nodes);
                if !seeds.is_empty() {
                    self.run_mounted_tail(snapshot.sql.as_str(), seeds, job.generation);
                }
                self.runtime_graph.mark_job_done(&job);
                handled += 1;
            }
        }
        handled
    }

    fn run_mounted_tail(&self, sql: &str, seeds: Vec<Arc<Cursor>>, generation: u64) {
        for inst in &self.instances {
            let Some(sql_idx) = inst.components.iter().position(|component| {
                component
                    .describe()
                    .and_then(|desc| desc.downcast_ref::<crate::sql::SqlQueryComponent>())
                    .map(|sql_component| sql_component.sql() == sql)
                    .unwrap_or(false)
            }) else {
                continue;
            };
            let tail = inst.components[sql_idx + 1..].iter().cloned().collect();
            let tail_inst = PipeInstance {
                pipe_hash: inst.pipe_hash,
                instance_id: inst.instance_id,
                components: tail,
            };
            expand(
                &tail_inst,
                self.queue.clone(),
                seeds,
                ExpandOpts::default()
                    .with_bus(self.bus.clone())
                    .with_runtime(self.runtime_graph.clone()),
            );
            self.sprf_store.flush();
            self.facts.commit(generation, Some(&self.bus));
            return;
        }
    }

    fn rerun_continuation_owner(&self, owner_uri_id: crate::StringId, generation: u64) -> bool {
        let Some(cont) = self.runtime_graph.continuation_for_owner(owner_uri_id) else {
            return false;
        };
        let Some(inst) = self
            .instances
            .iter()
            .find(|inst| inst.pipe_hash == cont.pipe_hash && inst.instance_id == cont.instance_id)
        else {
            return false;
        };
        let Some(comp) = inst.components.get(cont.depth as usize) else {
            return false;
        };
        let input = Arc::new(cont.input);
        let ctx = effect_runtime::v2::RenderCtx::new(cont.pipe_hash, cont.depth, generation)
            .with_bus(self.bus.clone())
            .with_runtime(self.runtime_graph.clone());
        let nodes = comp.render_batch(&ctx, &[input.as_ref()]);
        let seeds = node_seeds(&nodes);
        if !seeds.is_empty() {
            self.run_tail_from(inst, cont.depth + 1, seeds, generation);
        }
        true
    }

    fn run_tail_from(
        &self,
        inst: &PipeInstance<Cursor>,
        depth: u32,
        seeds: Vec<Arc<Cursor>>,
        generation: u64,
    ) {
        if depth as usize > inst.components.len() {
            return;
        }
        let tail = inst.components[depth as usize..].iter().cloned().collect();
        let tail_inst = PipeInstance {
            pipe_hash: inst.pipe_hash,
            instance_id: inst.instance_id,
            components: tail,
        };
        expand(
            &tail_inst,
            self.queue.clone(),
            seeds,
            ExpandOpts::default()
                .with_bus(self.bus.clone())
                .with_runtime(self.runtime_graph.clone()),
        );
        self.sprf_store.flush();
        self.facts.commit(generation, Some(&self.bus));
    }
}

fn node_seeds(nodes: &[Node<Cursor>]) -> Vec<Arc<Cursor>> {
    let mut out = Vec::new();
    for node in nodes {
        collect_node_seeds(node, &mut out);
    }
    out
}

fn collect_node_seeds(node: &Node<Cursor>, out: &mut Vec<Arc<Cursor>>) {
    match node {
        Node::Emit(cursor) => out.push(cursor.clone()),
        Node::Many(nodes) => {
            for node in nodes {
                collect_node_seeds(node, out);
            }
        }
        Node::Yield { value, .. } => out.push(value.clone()),
        Node::Done => {}
    }
}
