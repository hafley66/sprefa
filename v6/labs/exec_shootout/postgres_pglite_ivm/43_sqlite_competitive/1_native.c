/* Public SQLite extension ABI. Original experiment code, no upstream code copied. */
#include <sqlite3ext.h>
SQLITE_EXTENSION_INIT1
#include <stdio.h>
#include <string.h>
#include <time.h>

#include "0a_state.h"
#include "0ab_expressions.h"
#include "0b_delta.h"
#include "0bc_nonmonotone.h"
#include "0c_batch.h"

static void trace(Tab *t, const char *event, int depth) {
  Env *e = t->env;
  if (e->events >= 256) return;
  char line[96];
  int n = snprintf(line, sizeof(line), "%s[\"%s\",%d]",
                   e->events ? "," : "", event, depth);
  memcpy(e->trace + e->used, line, n + 1);
  e->used += n;
  e->events++;
  if (e->logging) fprintf(stderr, "{\"callback\":\"%s\",\"depth\":%d}\n", event, depth);
}

static int error(Tab *t, const char *msg) {
  sqlite3_free(t->base.zErrMsg);
  t->base.zErrMsg = sqlite3_mprintf("take2: %s", msg);
  return SQLITE_ERROR;
}

/* Every nested statement targets ordinary shadow storage. NO_VTAB prevents
 * accidental recursive virtual-table access through a substituted view. */
static int execute_sql(Tab *t, char *text, sqlite3_value **values, int n,sqlite3_int64 *scalar) {
  sqlite3_stmt *s = 0;
  if (!text) return SQLITE_NOMEM;
  struct timespec stamp;
  clock_gettime(CLOCK_MONOTONIC,&stamp);
  sqlite3_int64 start=(sqlite3_int64)stamp.tv_sec*1000000000+stamp.tv_nsec;
  if(t->env->cache) for(int i=0;i<32;i++) if(t->cached[i].text && !strcmp(t->cached[i].text,text)) {s=t->cached[i].stmt;break;}
  int rc=SQLITE_OK;
  if(!s) {
    rc = sqlite3_prepare_v3(t->db, text, -1, SQLITE_PREPARE_NO_VTAB|SQLITE_PREPARE_PERSISTENT, &s, 0);
    t->env->prepares++;
    if(rc==SQLITE_OK && t->env->cache) {
      Cached *c=&t->cached[t->next_cache++%32];
      sqlite3_finalize(c->stmt);sqlite3_free(c->text);
      c->stmt=s;c->text=text;text=0;
    }
  }
  clock_gettime(CLOCK_MONOTONIC,&stamp);
  t->env->prepare_ns+=(sqlite3_int64)stamp.tv_sec*1000000000+stamp.tv_nsec-start;
  sqlite3_free(text);
  for (int i = 0; rc == SQLITE_OK && i < n; i++) rc = sqlite3_bind_value(s, i+1, values[i]);
  clock_gettime(CLOCK_MONOTONIC,&stamp);start=(sqlite3_int64)stamp.tv_sec*1000000000+stamp.tv_nsec;
  if (rc == SQLITE_OK) {
    int step=sqlite3_step(s);
    rc = step == (scalar?SQLITE_ROW:SQLITE_DONE) ? SQLITE_OK : sqlite3_errcode(t->db);
    if(scalar&&rc==SQLITE_OK){if(sqlite3_column_type(s,0)!=SQLITE_INTEGER)rc=SQLITE_MISMATCH;else *scalar=sqlite3_column_int64(s,0);}
    if(scalar)t->env->scalar_steps++;
    t->env->steps++;
    t->env->vm+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_VM_STEP,1);
    t->env->scans+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_FULLSCAN_STEP,1);
    t->env->reprepares+=sqlite3_stmt_status(s,SQLITE_STMTSTATUS_REPREPARE,1);
  }
  clock_gettime(CLOCK_MONOTONIC,&stamp);t->env->step_ns+=(sqlite3_int64)stamp.tv_sec*1000000000+stamp.tv_nsec-start;
  int end;
  if(t->env->cache) {end=sqlite3_reset(s);sqlite3_clear_bindings(s);} else end=sqlite3_finalize(s);
  return rc == SQLITE_OK ? end : rc;
}
static int sql(Tab *t,char *text,sqlite3_value **values,int n){return execute_sql(t,text,values,n,0);}
static int scalar_sql(Tab *t,char *text,sqlite3_value **values,int n,sqlite3_int64 *out){return execute_sql(t,text,values,n,out);}

static int disconnect(sqlite3_vtab *v) {
  Tab *t = (Tab *)v;
  for(int i=0;i<32;i++) {sqlite3_finalize(t->cached[i].stmt);sqlite3_free(t->cached[i].text);}
  sqlite3_finalize(t->batch_read);
  sqlite3_free(t->predicate);sqlite3_free(t->projection);
  sqlite3_free(t->key_expression);for(int i=0;i<3;i++)sqlite3_free(t->aliases[i]);
  sqlite3_free(t->schema); sqlite3_free(t->name); sqlite3_free(t);
  return SQLITE_OK;
}

static int connect(sqlite3 *db, void *aux, int argc, const char *const *argv,
                   sqlite3_vtab **out, char **err) {
  (void)err;
  if (argc != 3 && argc != 4 && argc != 5 && argc != 6) return SQLITE_ERROR;
  Tab *t = sqlite3_malloc64(sizeof(*t));
  if (!t) return SQLITE_NOMEM;
  memset(t, 0, sizeof(*t));
  t->db = db; t->env = aux;
  t->frontiers=!strcmp(argv[0],"take2_epoch");
  t->fused=!strcmp(argv[0],"take2_counted")?2:!strcmp(argv[0],"take2_fused");
  t->lazy=!strcmp(argv[0],"take2_lazy")||t->fused;
  if(argc>=4) {
    const char *modes[]={"mirror","filter","bag","group","join","self","multi","project","inner","self_chain","chain","semi","anti","reach","distinct","fanout","diamond","plan","window"};
    int found=0;
    for(int i=0;i<19;i++) if(!strcmp(argv[3],modes[i])) { t->mode=i; found=1; }
    if(!found) { sqlite3_free(t); return SQLITE_ERROR; }
  }
  if((t->frontiers||t->lazy)&&!t->mode){sqlite3_free(t);*err=sqlite3_mprintf("epoch/lazy module requires a query mode");return SQLITE_ERROR;}
  if(t->mode==WINDOW) {
    char *width=argc==5?literal(argv[4]):0;
    int valid=width&&*width&&t->lazy;
    if(valid)for(const char *p=width;*p&&valid;p++){if(*p<'0'||*p>'9'||t->window_size>100000)valid=0;else t->window_size=t->window_size*10+*p-'0';}
    sqlite3_free(width);
    if(!valid||t->window_size<1||t->window_size>1000000){disconnect(&t->base);*err=sqlite3_mprintf("window requires lazy module and integer width 1..1000000");return SQLITE_ERROR;}
  } else if(t->mode==PLAN) {
    if(argc!=5||load_plan(t,argv[4])!=SQLITE_OK){disconnect(&t->base);*err=sqlite3_mprintf("take2: invalid bounded plan or scalar binding");return SQLITE_ERROR;}
  } else if(argc==5){disconnect(&t->base);return SQLITE_ERROR;}
  else if(t->mode==PROJECT||(t->mode==REACH&&argc==6)) {
    t->predicate=argc==6?literal(argv[4]):sqlite3_mprintf("b0.v>=0");
    t->projection=argc==6?literal(argv[5]):sqlite3_mprintf("b0.v*2");
    if(!t->predicate||!t->projection||validate_expressions(t)!=SQLITE_OK){disconnect(&t->base);*err=sqlite3_mprintf("take2: scalar plan requires k/v expressions without subqueries or unapproved functions");return SQLITE_ERROR;}
  } else if(argc==6){disconnect(&t->base);return SQLITE_ERROR;}
  t->schema = sqlite3_mprintf("%s", argv[1]);
  t->name = sqlite3_mprintf("%s", argv[2]);
  if (!t->schema || !t->name) { disconnect(&t->base); return SQLITE_NOMEM; }
  int rc = sqlite3_declare_vtab(db, "CREATE TABLE x(id INTEGER,k INTEGER,v INTEGER,op HIDDEN,old_id HIDDEN,old_k HIDDEN,old_v HIDDEN,side HIDDEN)");
  if (rc != SQLITE_OK) { disconnect(&t->base); return rc; }
  *out = &t->base;
  return SQLITE_OK;
}

static int create(sqlite3 *db, void *aux, int argc, const char *const *argv,
                  sqlite3_vtab **out, char **err) {
  int rc = connect(db, aux, argc, argv, out, err);
  if (rc != SQLITE_OK) return rc;
  Tab *t = (Tab *)*out;
  rc = sql(t, sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_state\"(id INTEGER,k INTEGER,v INTEGER,side INTEGER NOT NULL,PRIMARY KEY(side,id)) WITHOUT ROWID", t->schema,t->name),0,0);
  if(rc==SQLITE_OK) rc=sql(t,sqlite3_mprintf("CREATE INDEX \"%w\".\"%w_lookup\" ON \"%w_state\"(side,k)",t->schema,t->name,t->name),0,0);
  if(rc==SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_result\"(key TEXT PRIMARY KEY,k INTEGER,v INTEGER,n INTEGER,s INTEGER,nn INTEGER)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("CREATE INDEX \"%w\".\"%w_result_key\" ON \"%w_result\"(k)",t->schema,t->name,t->name),0,0);
  if(rc==SQLITE_OK && t->mode==REACH) rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_cone\"(k INTEGER PRIMARY KEY)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK) rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_sources\"(side INTEGER PRIMARY KEY,source TEXT UNIQUE,id_col TEXT,k_col TEXT,v_col TEXT)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_batch\"(flag INTEGER NOT NULL,source_views INTEGER NOT NULL)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("INSERT INTO \"%w\".\"%w_batch\" VALUES(0,0)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&(t->frontiers||t->mode==WINDOW))rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_clock\"(epoch INTEGER NOT NULL)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&(t->frontiers||t->mode==WINDOW))rc=sql(t,sqlite3_mprintf("INSERT INTO \"%w\".\"%w_clock\" VALUES(0)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&t->frontiers)rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_frontier\"(side INTEGER PRIMARY KEY,t INTEGER NOT NULL)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&t->frontiers)rc=sql(t,sqlite3_mprintf("INSERT INTO \"%w\".\"%w_frontier\" VALUES(0,0),(1,0),(2,0)",t->schema,t->name),0,0);
  if(rc==SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_delta\"(key TEXT PRIMARY KEY,side INTEGER,k INTEGER,v INTEGER,w INTEGER%s)",t->schema,t->name,t->fused?",events INTEGER NOT NULL DEFAULT 0":""),0,0);
  if(rc==SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("CREATE INDEX \"%w\".\"%w_delta_lookup\" ON \"%w_delta\"(side,k)",t->schema,t->name,t->name),0,0);
  if (rc == SQLITE_OK) rc = sql(t, sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_%s\"(n INTEGER NOT NULL);",t->schema,t->name,t->fused==2?"counter":"stats"),0,0);
  if (rc == SQLITE_OK) rc = sql(t, sqlite3_mprintf("INSERT INTO \"%w\".\"%w_%s\" VALUES(0)",t->schema,t->name,t->fused==2?"counter":"stats"),0,0);
  if(rc==SQLITE_OK&&t->fused==1)rc=sql(t,sqlite3_mprintf("CREATE TRIGGER \"%w\".\"%w_delta_ai\" AFTER INSERT ON \"%w_delta\" WHEN NEW.events>0 BEGIN UPDATE \"%w_stats\" SET n=min(1000000000000000,n+NEW.events); END",t->schema,t->name,t->name,t->name),0,0);
  if(rc==SQLITE_OK&&t->fused==1)rc=sql(t,sqlite3_mprintf("CREATE TRIGGER \"%w\".\"%w_delta_au\" AFTER UPDATE ON \"%w_delta\" WHEN NEW.events>OLD.events BEGIN UPDATE \"%w_stats\" SET n=min(1000000000000000,n+NEW.events-OLD.events); END",t->schema,t->name,t->name,t->name),0,0);
  if(rc==SQLITE_OK&&t->fused==2)rc=sql(t,sqlite3_mprintf("CREATE VIEW \"%w\".\"%w_stats\" AS SELECT min(1000000000000000,n+coalesce((SELECT sum(events) FROM \"%w_delta\"),0)) AS n FROM \"%w_counter\"",t->schema,t->name,t->name,t->name),0,0);
  if (rc != SQLITE_OK) { disconnect(*out); *out = 0; }
  return rc;
}
static int destroy(sqlite3_vtab *v) {
  Tab *t = (Tab *)v;
  int flag=0,rc=batching(t,&flag);
  sqlite3_stmt *s=0;
  char *q=sqlite3_mprintf("SELECT source FROM \"%w\".\"%w_sources\" ORDER BY side",t->schema,t->name);
  char *attached[3]={0};int count=0;
  if(rc==SQLITE_OK)rc=q?sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,0):SQLITE_NOMEM;
  sqlite3_free(q);
  while(rc==SQLITE_OK) {
    int step=sqlite3_step(s);if(step==SQLITE_DONE)break;
    if(step!=SQLITE_ROW){rc=step;break;}
    if(count==3){rc=error(t,"too many attached sources");break;}
    attached[count]=sqlite3_mprintf("%s",sqlite3_column_text(s,0));
    if(!attached[count++])rc=SQLITE_NOMEM;
  }
  sqlite3_finalize(s);
  sqlite3_finalize(t->batch_read);t->batch_read=0;
  for(int i=0;i<32;i++){sqlite3_finalize(t->cached[i].stmt);sqlite3_free(t->cached[i].text);t->cached[i].stmt=0;t->cached[i].text=0;}
  for(int i=0;i<count;i++) {
    const char *suffixes[]={"ai","ad","au"};
    for(int j=0;j<3&&rc==SQLITE_OK;j++)rc=sql(t,sqlite3_mprintf("DROP TRIGGER \"%w\".\"%w_%w_%s\"",t->schema,t->name,attached[i],suffixes[j]),0,0);
    sqlite3_free(attached[i]);
  }
  for(int i=0;i<sources(t)&&rc==SQLITE_OK&&t->source_view;i++) {
    rc=sql(t,sqlite3_mprintf("DROP VIEW \"%w\".\"%w_live_%d\"",t->schema,t->name,i),0,0);
    if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DROP INDEX \"%w\".\"%w_live_key_%d\"",t->schema,t->name,i),0,0);
  }
  if(rc==SQLITE_OK&&t->mode==REACH)rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_cone\"",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&t->frontiers)rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_frontier\"",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&(t->frontiers||t->mode==WINDOW))rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_clock\"",t->schema,t->name),0,0);
  if(rc==SQLITE_OK)rc = sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_state\"",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&t->source_view)rc=sql(t,sqlite3_mprintf("DROP VIEW \"%w\".\"%w_live\"",t->schema,t->name),0,0);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_sources\"",t->schema,t->name),0,0);
  if (rc == SQLITE_OK) rc = sql(t,sqlite3_mprintf("DROP %s \"%w\".\"%w_stats\"",t->fused==2?"VIEW":"TABLE",t->schema,t->name),0,0);
  if(rc==SQLITE_OK&&t->fused==2)rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_counter\"",t->schema,t->name),0,0);
  if (rc == SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_result\"",t->schema,t->name),0,0);
  if (rc == SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_delta\"",t->schema,t->name),0,0);
  if (rc == SQLITE_OK && t->mode) rc=sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_batch\"",t->schema,t->name),0,0);
  return rc == SQLITE_OK ? disconnect(v) : rc;
}
static int best(sqlite3_vtab *v, sqlite3_index_info *i) {
  (void)v; i->estimatedCost = 1000000; return SQLITE_OK;
}
static int open_cursor(sqlite3_vtab *v, sqlite3_vtab_cursor **out) {
  (void)v;
  Cursor *c = sqlite3_malloc64(sizeof(*c));
  if (!c) return SQLITE_NOMEM;
  memset(c,0,sizeof(*c)); *out = &c->base; return SQLITE_OK;
}
static int close_cursor(sqlite3_vtab_cursor *p) {
  Cursor *c = (Cursor *)p;
  sqlite3_finalize(c->stmt); sqlite3_free(c); return SQLITE_OK;
}
static int next(sqlite3_vtab_cursor *p) {
  Cursor *c = (Cursor *)p;
  c->ordinal++;
  if(c->repeats>1){c->repeats--;return SQLITE_OK;}
  int rc = sqlite3_step(c->stmt); c->eof = rc != SQLITE_ROW;
  if(rc==SQLITE_ROW&&((Tab *)p->pVtab)->mode>=PROJECT)c->repeats=sqlite3_column_int64(c->stmt,3);
  if(((Tab *)p->pVtab)->mode==DISTINCT)c->repeats=1;
  return rc == SQLITE_ROW || rc == SQLITE_DONE ? SQLITE_OK : rc;
}
static int filter(sqlite3_vtab_cursor *p, int idx, const char *str, int n, sqlite3_value **args) {
  (void)idx; (void)str; (void)n; (void)args;
  Cursor *c = (Cursor *)p; Tab *t = (Tab *)p->pVtab;
  trace(t,"filter",-1);
  int flag=0,check=batching(t,&flag);
  if(check!=SQLITE_OK)return check;
  if(flag&&t->lazy){check=flush_batch(t);if(check!=SQLITE_OK)return check;flag=0;}
  if(flag)return error(t,"batch open: flush before reading maintained output");
  sqlite3_finalize(c->stmt); c->stmt = 0;
  c->repeats=c->ordinal=0;
  char *q = sqlite3_mprintf("SELECT id,k,v FROM \"%w\".\"%w_state\" ORDER BY id", t->schema,t->name);
  if(t->mode) {
    sqlite3_free(q);
    q=sqlite3_mprintf("SELECT rowid,%s FROM \"%w\".\"%w_result\"",bag(t)||t->mode==REACH?"k,v,n":"k,n,CASE WHEN nn=0 THEN NULL ELSE s END",t->schema,t->name);
  }
  if (!q) return SQLITE_NOMEM;
  int rc = sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&c->stmt,0);
  sqlite3_free(q); return rc == SQLITE_OK ? next(p) : rc;
}
static int eof(sqlite3_vtab_cursor *p) { return ((Cursor *)p)->eof; }
static int column(sqlite3_vtab_cursor *p, sqlite3_context *ctx, int i) {
  if (i < 3) sqlite3_result_value(ctx,sqlite3_column_value(((Cursor *)p)->stmt,i+(((Tab *)p->pVtab)->mode?1:0)));
  else sqlite3_result_null(ctx);
  return SQLITE_OK;
}
static int rowid(sqlite3_vtab_cursor *p, sqlite3_int64 *id) {
  *id = ((Tab *)p->pVtab)->mode>=PROJECT?((Cursor *)p)->ordinal:sqlite3_column_int64(((Cursor *)p)->stmt,0); return SQLITE_OK;
}

static int update(sqlite3_vtab *v, int argc, sqlite3_value **a, sqlite3_int64 *id) {
  Tab *t = (Tab *)v;
  trace(t,"update",sqlite3_vtab_on_conflict(t->db));
  if (t->busy) return error(t,"recursive xUpdate rejected");
  if (argc != 10 || sqlite3_value_type(a[0]) != SQLITE_NULL)
    return error(t,"only forwarded event INSERT is supported");
  int op = sqlite3_value_int(a[5]);
  if(op==13) {
    if(!t->env->preparing)return error(t,"layout setup requires direct take2_prepare call");
    int flag=0,rc=batching(t,&flag);
    if(rc==SQLITE_OK&&flag)rc=error(t,"layout setup requires no pending batch");
    if(rc==SQLITE_OK)rc=source_view_setup(t);
    *id=0;return rc;
  }
  if(op==10||op==11) {*id=0;t->busy=1;int rc=batch_command(t,op);t->busy=0;return rc;}
  if(op==12){*id=0;t->busy=1;int rc=t->mode==WINDOW?advance_window(t,a[2]):seal_frontier(t,a[2],a[9]);t->busy=0;return rc;}
  if (op < 1 || op > 3) return error(t,"op must be 1 insert, 2 delete, 3 update");
  int side=sqlite3_value_int(a[9]);
  if(side<0||side>=sources(t)) return error(t,"invalid source side");
  for(int image=0;image<2;image++) {
    if((image==0 && op==2)||(image==1 && op==1)) continue;
    int start=image?6:2;
    if(sqlite3_value_type(a[start])!=SQLITE_INTEGER) return error(t,"integer row identity required");
    if(t->mode) for(int j=1;j<=2;j++) {
      int type=sqlite3_value_type(a[start+j]);
      if(t->mode==REACH&&type==SQLITE_NULL)return error(t,"reach requires non-NULL integer nodes");
      sqlite3_int64 n=sqlite3_value_int64(a[start+j]);
      if(type!=SQLITE_NULL && (type!=SQLITE_INTEGER||n < -1000000||n > 1000000)) return error(t,"NULL or integer in [-1000000,1000000] required");
    }
  }
  sqlite3_stmt *pragma=0;
  sqlite3_int64 started=now_ns();
  int rc=sqlite3_prepare_v2(t->db,"PRAGMA recursive_triggers",-1,&pragma,0);
  int enabled=rc==SQLITE_OK&&sqlite3_step(pragma)==SQLITE_ROW&&sqlite3_column_int(pragma,0);
  sqlite3_finalize(pragma);
  t->env->pragma_ns+=now_ns()-started;
  if (!enabled) return error(t,"recursive_triggers=ON required for REPLACE deletion forwarding");
  int batched=0;
  rc=batching(t,&batched);
  if(rc!=SQLITE_OK)return rc;
  if(t->mode==WINDOW&&op!=2&&(sqlite3_value_type(a[3])!=SQLITE_INTEGER||sqlite3_value_int64(a[3])<t->epoch-t->window_size+1||sqlite3_value_int64(a[3])>t->epoch))return error(t,"event timestamp outside admitted window");
  if(t->lazy&&!batched) {
    if(t->env->source_views&&!t->source_view)return error(t,"source-view layout requires take2_prepare before source writes");
    if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_batch\" SET flag=1",t->schema,t->name),0,0);
    if(rc!=SQLITE_OK)return rc;
    batched=1;
  }
  if((t->mode>=PROJECT||t->frontiers)&&!batched)return error(t,"circuit mode requires an open batch");
  if(t->frontiers){rc=frontier_check(t,side,0);if(rc!=SQLITE_OK)return rc;}
  if(t->source_view && (!t->env->source_views||!batched))return error(t,"source-view layout requires source_views_on and an open batch");
  t->busy = 1;
  if (op != 1) {
    if(!t->source_view) {
      rc = sql(t,sqlite3_mprintf("DELETE FROM \"%w\".\"%w_state\" WHERE id=?1 AND k IS ?2 AND v IS ?3 AND side=%d",t->schema,t->name,side),a+6,3);
      if (rc == SQLITE_OK && sqlite3_changes(t->db) != 1) rc = error(t,"OLD shadow mismatch");
    }
    if(rc==SQLITE_OK) rc=batched?enqueue(t,a+7,side,-1,op==2):contribute(t,a+7,side,-1);
  }
  if (rc == SQLITE_OK && op != 2) rc=batched?enqueue(t,a+3,side,1,1):contribute(t,a+3,side,1);
  if (rc == SQLITE_OK && op != 2 && !t->source_view)
    rc = sql(t,sqlite3_mprintf("INSERT INTO \"%w\".\"%w_state\" VALUES(?1,?2,?3,%d)",t->schema,t->name,side),a+2,3);
  started=now_ns();
  if (rc == SQLITE_OK&&!t->fused)
    rc = sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_stats\" SET n=min(1000000000000000,n+1)",t->schema,t->name),0,0);
  t->env->stats_ns+=now_ns()-started;
  t->busy = 0; *id = 0;
  return rc;
}
static int begin(sqlite3_vtab *v) { trace((Tab *)v,"begin",-1); return SQLITE_OK; }
static int sync_tab(sqlite3_vtab *v) {
  Tab *t = (Tab *)v; trace(t,"sync",-1);
  int flag=0,rc=batching(t,&flag);
  if(rc!=SQLITE_OK)return rc;
  if(flag&&t->lazy){rc=flush_batch(t);if(rc!=SQLITE_OK)return rc;flag=0;}
  if(flag)return error(t,"batch open at COMMIT: explicit flush required");
  if (t->env->fail_sync) { t->env->fail_sync = 0; return error(t,"injected xSync failure"); }
  return SQLITE_OK;
}
static int commit(sqlite3_vtab *v) { trace((Tab *)v,"commit",-1); return SQLITE_OK; }
static int rollback(sqlite3_vtab *v) { trace((Tab *)v,"rollback",-1); return SQLITE_OK; }
static int savepoint(sqlite3_vtab *v,int i) { trace((Tab *)v,"savepoint",i); return SQLITE_OK; }
static int release(sqlite3_vtab *v,int i) { trace((Tab *)v,"release",i); return SQLITE_OK; }
static int rollbackto(sqlite3_vtab *v,int i) { trace((Tab *)v,"rollbackto",i); return SQLITE_OK; }
static int shadow(const char *suffix) { return !strcmp(suffix,"state") || !strcmp(suffix,"stats") || !strcmp(suffix,"counter") || !strcmp(suffix,"result") || !strcmp(suffix,"batch") || !strcmp(suffix,"delta") || !strcmp(suffix,"sources") || !strcmp(suffix,"cone") || !strcmp(suffix,"clock") || !strcmp(suffix,"frontier"); }

static const sqlite3_module module = {
  .iVersion=3, .xCreate=create, .xConnect=connect, .xBestIndex=best,
  .xDisconnect=disconnect, .xDestroy=destroy, .xOpen=open_cursor,
  .xClose=close_cursor, .xFilter=filter, .xNext=next, .xEof=eof,
  .xColumn=column, .xRowid=rowid, .xUpdate=update,
  .xBegin=begin, .xSync=sync_tab, .xCommit=commit, .xRollback=rollback,
  .xSavepoint=savepoint, .xRelease=release, .xRollbackTo=rollbackto, .xShadowName=shadow
};
static void control(sqlite3_context *ctx,int argc,sqlite3_value **a) {
  (void)argc;
  Env *e = sqlite3_user_data(ctx);
  const char *cmd = (const char *)sqlite3_value_text(a[0]);
  if (cmd && !strcmp(cmd,"trace")) {
    char *json = sqlite3_mprintf("[%s]",e->trace);
    sqlite3_result_text(ctx,json,-1,sqlite3_free);
    e->used = e->events = 0; e->trace[0] = 0;
  } else if (cmd && !strcmp(cmd,"fail_sync")) e->fail_sync = 1;
  else if (cmd && !strcmp(cmd,"cache_on")) e->cache=1;
  else if (cmd && !strcmp(cmd,"source_views_on")) e->source_views=1;
  else if (cmd && !strcmp(cmd,"profile_reset")) e->prepares=e->steps=e->vm=e->scans=e->prepare_ns=e->step_ns=e->delta_build_ns=e->reprepares=e->scalar_steps=e->batch_read_ns=e->pragma_ns=e->enqueue_ns=e->stats_ns=e->flush_ns=0;
  else if (cmd && !strcmp(cmd,"profile")) sqlite3_result_text(ctx,sqlite3_mprintf("{\"prepares\":%lld,\"steps\":%lld,\"vm_steps\":%lld,\"fullscan_steps\":%lld,\"prepare_ns\":%lld,\"step_ns\":%lld,\"delta_sql_build_ns\":%lld,\"automatic_reprepares\":%lld,\"scalar_steps\":%lld,\"batch_read_ns\":%lld,\"pragma_ns\":%lld,\"enqueue_ns\":%lld,\"stats_ns\":%lld,\"flush_ns\":%lld}",e->prepares,e->steps,e->vm,e->scans,e->prepare_ns,e->step_ns,e->delta_build_ns,e->reprepares,e->scalar_steps,e->batch_read_ns,e->pragma_ns,e->enqueue_ns,e->stats_ns,e->flush_ns),-1,sqlite3_free);
  else if (cmd && !strcmp(cmd,"log_on")) e->logging = 1;
  else if (cmd && !strcmp(cmd,"log_off")) e->logging = 0;
  else sqlite3_result_error(ctx,"unknown take2 control",-1);
}
static void release_env(void *p){Env *e=p;if(--e->references==0)sqlite3_free(e);}
static void prepare_layout(sqlite3_context *ctx,int argc,sqlite3_value **a) {
  (void)argc;Env *e=sqlite3_user_data(ctx);
  const char *name=(const char *)sqlite3_value_text(a[0]);
  if(!name||e->preparing){sqlite3_result_error(ctx,"invalid/reentrant layout preparation",-1);return;}
  char *q=sqlite3_mprintf("INSERT INTO main.\"%w\"(op) VALUES(13)",name),*err=0;
  e->preparing=1;int rc=q?sqlite3_exec(sqlite3_context_db_handle(ctx),q,0,0,&err):SQLITE_NOMEM;e->preparing=0;
  if(rc!=SQLITE_OK)sqlite3_result_error(ctx,err?err:"layout preparation failed",-1);
  sqlite3_free(q);sqlite3_free(err);
}
#ifdef _WIN32
__declspec(dllexport)
#endif
int sqlite3_extension_init(sqlite3 *db,char **err,const sqlite3_api_routines *api) {
  (void)err; SQLITE_EXTENSION_INIT2(api);
  Env *e = sqlite3_malloc64(sizeof(*e));
  if (!e) return SQLITE_NOMEM;
  memset(e,0,sizeof(*e));
  e->references=1;
  int rc = sqlite3_create_module_v2(db,"take2",&module,e,release_env);
  if(rc!=SQLITE_OK)return rc;
  e->references++;
  rc=sqlite3_create_module_v2(db,"take2_epoch",&module,e,release_env);
  if(rc!=SQLITE_OK)return rc;
  e->references++;
  rc=sqlite3_create_module_v2(db,"take2_lazy",&module,e,release_env);
  if(rc!=SQLITE_OK)return rc;
  e->references++;
  rc=sqlite3_create_module_v2(db,"take2_fused",&module,e,release_env);
  if(rc!=SQLITE_OK)return rc;
  e->references++;
  rc=sqlite3_create_module_v2(db,"take2_counted",&module,e,release_env);
  if (rc == SQLITE_OK) rc = sqlite3_create_function_v2(db,"take2_control",1,SQLITE_UTF8, e,control,0,0,0);
  if (rc == SQLITE_OK) rc = sqlite3_create_function_v2(db,"take2_attach",6,SQLITE_UTF8|SQLITE_DIRECTONLY,0,attach,0,0,0);
  if (rc == SQLITE_OK) rc = sqlite3_create_function_v2(db,"take2_prepare",1,SQLITE_UTF8|SQLITE_DIRECTONLY,e,prepare_layout,0,0,0);
  return rc;
}
