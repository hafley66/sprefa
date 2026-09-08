/* Signed contribution accumulator adapted from Take 1's 2_lower.rs.
 * The input is one OLD/NEW tuple; state scans bind only the affected join key.
 * SQL parsing/binding is SQLite's. Mode selection is explicit, not SQL parsing. */
static int contribute(Tab *t, sqlite3_value **kv, int side, int sign) {
  if (!t->mode) return SQLITE_OK;
  int arity = t->mode == MULTI ? 3 : t->mode >= JOIN ? 2 : 1;
  int masks = t->mode == SELF ? 3 : 1;
  int rc = SQLITE_OK;
  for (int term=1; term<=masks && rc==SQLITE_OK; term++) {
    sqlite3_int64 build_start=now_ns();
    int mask = t->mode == SELF ? term : 1 << side;
    sqlite3_str *from = sqlite3_str_new(t->db);
    for (int i=0; i<arity; i++) {
      if (i) sqlite3_str_appendall(from," JOIN ");
      if (mask & (1<<i)) sqlite3_str_appendf(from,"d b%d",i);
      else sqlite3_str_appendf(from,"(SELECT k,v FROM \"%w\".\"%w_state\" WHERE side=%d AND k=?1) b%d",t->schema,t->name,t->mode==SELF?0:i,i);
      if (i) sqlite3_str_appendf(from," ON b0.k=b%d.k",i);
    }
    char *relations = sqlite3_str_finish(from);
    const char *value = arity == 3 ? "b0.v*b1.v*b2.v" : arity == 2 ? "b0.v*b1.v" : "b0.v";
    const char *key = t->mode <= BAG ? "json_array(b0.k,b0.v)" : "json_array(b0.k)";
    const char *projection = t->mode <= BAG ? "b0.v" : "NULL";
    const char *group = t->mode <= BAG ? "b0.k,b0.v" : "b0.k";
    char *statement = sqlite3_mprintf(
      "WITH d(k,v) AS(VALUES(?1,?2)) INSERT INTO \"%w\".\"%w_result\"(key,k,v,n,s,nn) "
      "SELECT %s,b0.k,%s,%d*count(*),%d*coalesce(sum(%s),0),%d*count(%s) FROM %s %s GROUP BY %s "
      "ON CONFLICT(key) DO UPDATE SET n=n+excluded.n,s=s+excluded.s,nn=nn+excluded.nn",
      t->schema,t->name,key,projection,sign,sign,value,sign,value,relations,
      t->mode==FILTER?"WHERE b0.v>=0":"",group);
    sqlite3_free(relations);
    t->env->delta_build_ns+=now_ns()-build_start;
    rc = sql(t,statement,kv,2);
  }
  if (rc == SQLITE_OK) {
    char *q = sqlite3_mprintf("SELECT count(*) FROM \"%w\".\"%w_result\" WHERE k IS ?1 AND (n<0 OR nn<0 OR nn>n OR typeof(n)<>'integer' OR typeof(s)<>'integer' OR typeof(nn)<>'integer' OR (n=0 AND (s<>0 OR nn<>0)))",t->schema,t->name);
    sqlite3_int64 bad=0;
    rc=scalar_sql(t,q,kv,1,&bad);
    if(rc==SQLITE_OK&&bad)rc=error(t,"accumulator integer overflow or support invariant");
  }
  if (rc==SQLITE_OK) rc=sql(t,sqlite3_mprintf("DELETE FROM \"%w\".\"%w_result\" WHERE k IS ?1 AND n=0",t->schema,t->name),kv,1);
  return rc;
}

/* Installing transport is separate from delta scheduling. Sources must be empty
 * when attached; loading them afterwards is ordinary DML through these triggers. */
static void attach(sqlite3_context *ctx,int argc,sqlite3_value **a) {
  (void)argc;
  sqlite3 *db=sqlite3_context_db_handle(ctx);
  const char *view=(const char *)sqlite3_value_text(a[0]);
  const char *source=(const char *)sqlite3_value_text(a[1]);
  int side=sqlite3_value_int(a[2]);
  const char *id=(const char *)sqlite3_value_text(a[3]);
  const char *k=(const char *)sqlite3_value_text(a[4]);
  const char *v=(const char *)sqlite3_value_text(a[5]);
  if (!view||!source||!id||!k||!v||side<0||side>2) { sqlite3_result_error(ctx,"take2: invalid attachment",-1); return; }
  char *q=sqlite3_mprintf("SELECT 1 FROM main.\"%w\" LIMIT 1",source);
  sqlite3_stmt *s=0;
  int rc=q?sqlite3_prepare_v3(db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,0):SQLITE_NOMEM;
  sqlite3_free(q);
  if (rc==SQLITE_OK && sqlite3_step(s)!=SQLITE_DONE) rc=SQLITE_ERROR;
  sqlite3_finalize(s);
  if (rc!=SQLITE_OK) { sqlite3_result_error(ctx,"take2: attach requires an empty ordinary source",-1); return; }
  q=sqlite3_mprintf(
    "INSERT INTO main.\"%w_sources\" VALUES(%d,%Q,%Q,%Q,%Q);"
    "CREATE TRIGGER main.\"%w_%w_ai\" AFTER INSERT ON \"%w\" BEGIN "
    "INSERT INTO \"%w\"(id,k,v,op,side) VALUES(NEW.\"%w\",NEW.\"%w\",NEW.\"%w\",1,%d); END;"
    "CREATE TRIGGER main.\"%w_%w_ad\" AFTER DELETE ON \"%w\" BEGIN "
    "INSERT INTO \"%w\"(op,old_id,old_k,old_v,side) VALUES(2,OLD.\"%w\",OLD.\"%w\",OLD.\"%w\",%d); END;"
    "CREATE TRIGGER main.\"%w_%w_au\" AFTER UPDATE ON \"%w\" BEGIN "
    "INSERT INTO \"%w\"(id,k,v,op,old_id,old_k,old_v,side) VALUES(NEW.\"%w\",NEW.\"%w\",NEW.\"%w\",3,OLD.\"%w\",OLD.\"%w\",OLD.\"%w\",%d); END;",
    view,side,source,id,k,v,
    view,source,source,view,id,k,v,side,view,source,source,view,id,k,v,side,
    view,source,source,view,id,k,v,id,k,v,side);
  char *err=0;
  rc=sqlite3_exec(db,"SAVEPOINT take2_attach_install",0,0,&err);
  if(rc==SQLITE_OK) {
    rc=q?sqlite3_exec(db,q,0,0,&err):SQLITE_NOMEM;
    if(rc!=SQLITE_OK) sqlite3_exec(db,"ROLLBACK TO take2_attach_install",0,0,0);
    int end=sqlite3_exec(db,"RELEASE take2_attach_install",0,0,0);
    if(rc==SQLITE_OK) rc=end;
  }
  sqlite3_free(q);
  if(rc!=SQLITE_OK) sqlite3_result_error(ctx,err?err:"take2 attachment allocation failure",-1);
  sqlite3_free(err);
}
