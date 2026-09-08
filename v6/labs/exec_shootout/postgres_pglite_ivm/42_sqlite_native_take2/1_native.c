/* Public SQLite extension ABI. Original experiment code, no upstream code copied. */
#include <sqlite3ext.h>
SQLITE_EXTENSION_INIT1
#include <stdio.h>
#include <string.h>

typedef struct Env {
  char trace[16384];
  int used, events, logging, fail_sync;
} Env;
typedef struct Tab {
  sqlite3_vtab base;
  sqlite3 *db;
  Env *env;
  char *schema, *name;
  int busy;
} Tab;
typedef struct Cursor {
  sqlite3_vtab_cursor base;
  sqlite3_stmt *stmt;
  int eof;
} Cursor;

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
static int sql(Tab *t, char *text, sqlite3_value **values, int n) {
  sqlite3_stmt *s = 0;
  if (!text) return SQLITE_NOMEM;
  int rc = sqlite3_prepare_v3(t->db, text, -1, SQLITE_PREPARE_NO_VTAB, &s, 0);
  sqlite3_free(text);
  for (int i = 0; rc == SQLITE_OK && i < n; i++) rc = sqlite3_bind_value(s, i+1, values[i]);
  if (rc == SQLITE_OK) rc = sqlite3_step(s) == SQLITE_DONE ? SQLITE_OK : sqlite3_errcode(t->db);
  int end = sqlite3_finalize(s);
  return rc == SQLITE_OK ? end : rc;
}

static int disconnect(sqlite3_vtab *v) {
  Tab *t = (Tab *)v;
  sqlite3_free(t->schema); sqlite3_free(t->name); sqlite3_free(t);
  return SQLITE_OK;
}

static int connect(sqlite3 *db, void *aux, int argc, const char *const *argv,
                   sqlite3_vtab **out, char **err) {
  (void)err;
  if (argc != 3) return SQLITE_ERROR;
  Tab *t = sqlite3_malloc64(sizeof(*t));
  if (!t) return SQLITE_NOMEM;
  memset(t, 0, sizeof(*t));
  t->db = db; t->env = aux;
  t->schema = sqlite3_mprintf("%s", argv[1]);
  t->name = sqlite3_mprintf("%s", argv[2]);
  if (!t->schema || !t->name) { disconnect(&t->base); return SQLITE_NOMEM; }
  int rc = sqlite3_declare_vtab(db, "CREATE TABLE x(id INTEGER,k INTEGER,v INTEGER,op HIDDEN,old_id HIDDEN,old_k HIDDEN,old_v HIDDEN)");
  if (rc != SQLITE_OK) { disconnect(&t->base); return rc; }
  *out = &t->base;
  return SQLITE_OK;
}

static int create(sqlite3 *db, void *aux, int argc, const char *const *argv,
                  sqlite3_vtab **out, char **err) {
  int rc = connect(db, aux, argc, argv, out, err);
  if (rc != SQLITE_OK) return rc;
  Tab *t = (Tab *)*out;
  rc = sql(t, sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_state\"(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)", t->schema,t->name),0,0);
  if (rc == SQLITE_OK) rc = sql(t, sqlite3_mprintf("CREATE TABLE \"%w\".\"%w_stats\"(n INTEGER NOT NULL);",t->schema,t->name),0,0);
  if (rc == SQLITE_OK) rc = sql(t, sqlite3_mprintf("INSERT INTO \"%w\".\"%w_stats\" VALUES(0)",t->schema,t->name),0,0);
  if (rc != SQLITE_OK) { disconnect(*out); *out = 0; }
  return rc;
}
static int destroy(sqlite3_vtab *v) {
  Tab *t = (Tab *)v;
  int rc = sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_state\"",t->schema,t->name),0,0);
  if (rc == SQLITE_OK) rc = sql(t,sqlite3_mprintf("DROP TABLE \"%w\".\"%w_stats\"",t->schema,t->name),0,0);
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
  int rc = sqlite3_step(c->stmt); c->eof = rc != SQLITE_ROW;
  return rc == SQLITE_ROW || rc == SQLITE_DONE ? SQLITE_OK : rc;
}
static int filter(sqlite3_vtab_cursor *p, int idx, const char *str, int n, sqlite3_value **args) {
  (void)idx; (void)str; (void)n; (void)args;
  Cursor *c = (Cursor *)p; Tab *t = (Tab *)p->pVtab;
  trace(t,"filter",-1);
  sqlite3_finalize(c->stmt); c->stmt = 0;
  char *q = sqlite3_mprintf("SELECT id,k,v FROM \"%w\".\"%w_state\" ORDER BY id", t->schema,t->name);
  if (!q) return SQLITE_NOMEM;
  int rc = sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&c->stmt,0);
  sqlite3_free(q); return rc == SQLITE_OK ? next(p) : rc;
}
static int eof(sqlite3_vtab_cursor *p) { return ((Cursor *)p)->eof; }
static int column(sqlite3_vtab_cursor *p, sqlite3_context *ctx, int i) {
  if (i < 3) sqlite3_result_value(ctx,sqlite3_column_value(((Cursor *)p)->stmt,i));
  else sqlite3_result_null(ctx);
  return SQLITE_OK;
}
static int rowid(sqlite3_vtab_cursor *p, sqlite3_int64 *id) {
  *id = sqlite3_column_int64(((Cursor *)p)->stmt,0); return SQLITE_OK;
}

static int update(sqlite3_vtab *v, int argc, sqlite3_value **a, sqlite3_int64 *id) {
  Tab *t = (Tab *)v;
  trace(t,"update",sqlite3_vtab_on_conflict(t->db));
  if (t->busy) return error(t,"recursive xUpdate rejected");
  if (argc != 9 || sqlite3_value_type(a[0]) != SQLITE_NULL)
    return error(t,"only forwarded event INSERT is supported");
  int op = sqlite3_value_int(a[5]);
  if (op < 1 || op > 3) return error(t,"op must be 1 insert, 2 delete, 3 update");
  sqlite3_stmt *pragma = 0;
  int rc = sqlite3_prepare_v2(t->db,"PRAGMA recursive_triggers",-1,&pragma,0);
  int enabled = rc == SQLITE_OK && sqlite3_step(pragma) == SQLITE_ROW && sqlite3_column_int(pragma,0);
  sqlite3_finalize(pragma);
  if (!enabled) return error(t,"recursive_triggers=ON required for REPLACE deletion forwarding");
  t->busy = 1;
  if (op != 1) {
    rc = sql(t,sqlite3_mprintf("DELETE FROM \"%w\".\"%w_state\" WHERE id=?1 AND k IS ?2 AND v IS ?3",t->schema,t->name),a+6,3);
    if (rc == SQLITE_OK && sqlite3_changes(t->db) != 1) rc = error(t,"OLD shadow mismatch");
  }
  if (rc == SQLITE_OK && op != 2)
    rc = sql(t,sqlite3_mprintf("INSERT INTO \"%w\".\"%w_state\" VALUES(?1,?2,?3)",t->schema,t->name),a+2,3);
  if (rc == SQLITE_OK)
    rc = sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_stats\" SET n=n+1",t->schema,t->name),0,0);
  t->busy = 0; *id = 0;
  return rc;
}
static int begin(sqlite3_vtab *v) { trace((Tab *)v,"begin",-1); return SQLITE_OK; }
static int sync_tab(sqlite3_vtab *v) {
  Tab *t = (Tab *)v; trace(t,"sync",-1);
  if (t->env->fail_sync) { t->env->fail_sync = 0; return error(t,"injected xSync failure"); }
  return SQLITE_OK;
}
static int commit(sqlite3_vtab *v) { trace((Tab *)v,"commit",-1); return SQLITE_OK; }
static int rollback(sqlite3_vtab *v) { trace((Tab *)v,"rollback",-1); return SQLITE_OK; }
static int savepoint(sqlite3_vtab *v,int i) { trace((Tab *)v,"savepoint",i); return SQLITE_OK; }
static int release(sqlite3_vtab *v,int i) { trace((Tab *)v,"release",i); return SQLITE_OK; }
static int rollbackto(sqlite3_vtab *v,int i) { trace((Tab *)v,"rollbackto",i); return SQLITE_OK; }
static int shadow(const char *suffix) { return !strcmp(suffix,"state") || !strcmp(suffix,"stats"); }

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
  else if (cmd && !strcmp(cmd,"log_on")) e->logging = 1;
  else if (cmd && !strcmp(cmd,"log_off")) e->logging = 0;
  else sqlite3_result_error(ctx,"unknown take2 control",-1);
}
#ifdef _WIN32
__declspec(dllexport)
#endif
int sqlite3_extension_init(sqlite3 *db,char **err,const sqlite3_api_routines *api) {
  (void)err; SQLITE_EXTENSION_INIT2(api);
  Env *e = sqlite3_malloc64(sizeof(*e));
  if (!e) return SQLITE_NOMEM;
  memset(e,0,sizeof(*e));
  int rc = sqlite3_create_module_v2(db,"take2",&module,e,sqlite3_free);
  if (rc == SQLITE_OK) rc = sqlite3_create_function_v2(db,"take2_control",1,SQLITE_UTF8, e,control,0,0,0);
  return rc;
}
