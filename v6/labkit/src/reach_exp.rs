use crate::reach::{dec, initial_edges, reach_digest, MUL};
use crate::store_db::StoreDb;
use crate::{Complexity, Experiment};
use std::collections::{HashMap, HashSet};

#[derive(Default)] pub struct RamReach { weight: HashMap<i64,i64>, last_digest:i64, last_card:u64, recomputes:u64 }
impl RamReach { fn recompute(&mut self) { let live:HashSet<i64>=self.weight.iter().filter(|(_,w)|**w>0).map(|(k,_)|*k).collect(); (self.last_digest,self.last_card)=reach_digest(&live); self.recomputes+=1; } }
impl Experiment for RamReach { fn name(&self)->&'static str{"ram-reach"} fn complexity(&self)->Complexity{Complexity{time:"O(V·E)/tick",space:"O(facts) resident"}} fn rationale(&self)->&'static str{"resident all-pairs recompute"} fn reset(&mut self){*self=Self::default()} fn setup(&mut self,b:usize){for k in initial_edges(b){*self.weight.entry(k).or_insert(0)+=1}self.recompute()} fn tick(&mut self,a:&[i64],r:&[i64]){for &k in a{*self.weight.entry(k).or_insert(0)+=1}for &k in r{if let Some(w)=self.weight.get_mut(&k){*w-=1;if *w<=0{self.weight.remove(&k);}}}self.recompute()} fn digest(&self)->i64{self.last_digest} fn live(&self)->u64{self.last_card} fn recompute_units(&self)->u64{self.recomputes} }

pub struct SqliteReach { db:StoreDb, writes:u64, recomputes:u64, last:i64, card:u64 }
impl Default for SqliteReach { fn default()->Self { let db=StoreDb::memory(); db.exec("CREATE TABLE fact(key INTEGER PRIMARY KEY, weight INTEGER NOT NULL)"); Self{db,writes:0,recomputes:0,last:0,card:0} } }
impl SqliteReach {
 fn apply(&mut self, ds:&[(i64,i64)]) { if ds.is_empty(){return} let values=ds.iter().map(|(k,w)|format!("({k},{w})")).collect::<Vec<_>>().join(","); self.db.exec(format!("INSERT INTO fact(key,weight) VALUES {values} ON CONFLICT(key) DO UPDATE SET weight=weight+excluded.weight; DELETE FROM fact WHERE weight<=0")); self.writes+=2; }
 fn recompute(&mut self) { let q=format!("WITH RECURSIVE r(src,cur) AS (SELECT key/{MUL},key%{MUL} FROM fact UNION SELECT r.src,f.key%{MUL} FROM r JOIN fact f ON f.key>=r.cur*{MUL} AND f.key<(r.cur+1)*{MUL}) SELECT src,cur FROM r"); let rows=self.db.rows(q); self.last=0; self.card=rows.len() as u64; for row in rows { let s:i64=row.try_get_by_index(0).unwrap(); let c:i64=row.try_get_by_index(1).unwrap(); self.last^=crate::mix(s*MUL+c); } self.recomputes+=1; }
}
impl Experiment for SqliteReach { fn name(&self)->&'static str{"sqlite-reach"} fn complexity(&self)->Complexity{Complexity{time:"O(reach)/tick recompute",space:"O(working set)"}} fn rationale(&self)->&'static str{"SQLite recursive CTE closure"} fn reset(&mut self){*self=Self::default()} fn setup(&mut self,b:usize){self.apply(&initial_edges(b).into_iter().map(|k|(k,1)).collect::<Vec<_>>());self.recompute()} fn tick(&mut self,a:&[i64],r:&[i64]){let mut d=a.iter().map(|k|(*k,1)).collect::<Vec<_>>();d.extend(r.iter().map(|k|(*k,-1)));self.apply(&d);self.recompute()} fn digest(&self)->i64{self.last} fn live(&self)->u64{self.card} fn recompute_units(&self)->u64{self.recomputes} fn writes(&self)->u64{self.writes} fn plan_snapshot(&self)->Option<String>{let _=dec(0);Some("    recursive CTE over fact\n".into())} }
