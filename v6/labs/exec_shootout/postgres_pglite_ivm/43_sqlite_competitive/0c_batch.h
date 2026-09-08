/* Explicit transaction-scoped batch. SQLite stores flag, deltas and all images.
 * Precommit view reads fail while open; flushing is an ordinary vtab INSERT. */
static int batching(Tab *t,int *flag) {
  *flag=0;
  if(!t->mode) return SQLITE_OK;
  int rc=SQLITE_OK;
  if(!t->batch_read) {
    char *q=t->frontiers?sqlite3_mprintf("SELECT flag,source_views,epoch FROM \"%w\".\"%w_batch\" CROSS JOIN \"%w\".\"%w_clock\"",t->schema,t->name,t->schema,t->name):sqlite3_mprintf("SELECT flag,source_views FROM \"%w\".\"%w_batch\"",t->schema,t->name);
    rc=q?sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB|SQLITE_PREPARE_PERSISTENT,&t->batch_read,0):SQLITE_NOMEM;
    sqlite3_free(q);
  }
  if(rc==SQLITE_OK) {
    rc=sqlite3_step(t->batch_read);
    if(rc==SQLITE_ROW) {*flag=sqlite3_column_int(t->batch_read,0);t->source_view=sqlite3_column_int(t->batch_read,1);if(t->frontiers)t->epoch=sqlite3_column_int64(t->batch_read,2);rc=SQLITE_OK;}
    else if(rc==SQLITE_DONE) rc=error(t,"missing batch flag");
  }
  sqlite3_reset(t->batch_read);
  return rc;
}
static int frontier_check(Tab *t,int side,int sealing) {
  sqlite3_stmt *s=0;
  char *q=side<0?sqlite3_mprintf("SELECT min(t) FROM \"%w\".\"%w_frontier\"",t->schema,t->name):sqlite3_mprintf("SELECT t FROM \"%w\".\"%w_frontier\" WHERE side=%d",t->schema,t->name,side);
  int rc=q?sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,0):SQLITE_NOMEM;sqlite3_free(q);
  if(rc==SQLITE_OK) {
    int step=sqlite3_step(s);
    if(step!=SQLITE_ROW||sqlite3_column_type(s,0)!=SQLITE_INTEGER)rc=error(t,"missing input frontier");
    else if(side<0&&sqlite3_column_int64(s,0)!=t->epoch)rc=error(t,"all input frontiers must advance before flush");
    else if(side>=0&&sqlite3_column_int64(s,0)>=t->epoch)rc=error(t,sealing?"frontier must strictly advance":"sealed input rejects late writes");
  }
  sqlite3_finalize(s);return rc;
}
static int seal_frontier(Tab *t,sqlite3_value *epoch,sqlite3_value *side_value) {
  int flag=0,rc=batching(t,&flag),side=sqlite3_value_int(side_value);
  if(rc!=SQLITE_OK)return rc;
  if(!t->frontiers||!flag||sqlite3_value_type(epoch)!=SQLITE_INTEGER||sqlite3_value_int64(epoch)!=t->epoch||sqlite3_value_type(side_value)!=SQLITE_INTEGER||side<0||side>2)return error(t,"frontier seal requires open epoch and input side 0..2");
  rc=frontier_check(t,side,1);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_frontier\" SET t=%lld WHERE side=%d",t->schema,t->name,t->epoch,side),0,0);
  return rc;
}
static int enqueue(Tab *t,sqlite3_value **kv,int side,int sign) {
  return sql(t,sqlite3_mprintf("INSERT INTO \"%w\".\"%w_delta\"(key,side,k,v,w) VALUES(json_array(%d,?1,?2),%d,?1,?2,%d) ON CONFLICT(key) DO UPDATE SET w=w+excluded.w",t->schema,t->name,side,side,sign),kv,2);
}
static int flush_batch(Tab *t) {
  int degree=arity(t);
  int rc=SQLITE_OK;
  if(t->mode==PROJECT)rc=projection_domain(t);
  if(t->mode==SEMI||t->mode==ANTI)rc=flush_membership(t);
  if(t->mode==REACH)rc=flush_reach(t);
  for(int branch=0;branch<(t->mode==DIAMOND?2:1);branch++)
  for(int mask=1;mask<(1<<degree)&&rc==SQLITE_OK&&t->mode!=SEMI&&t->mode!=ANTI&&t->mode!=REACH;mask++) {
    sqlite3_str *from=sqlite3_str_new(t->db);
    sqlite3_str *weight=sqlite3_str_new(t->db);
    int bits=0;
    for(int i=0;i<degree;i++) {
      int side=t->mode==SELF||t->mode==SELF_CHAIN?0:i;
      if(t->mode==PLAN)side=t->side_map[i];
      char *alias=t->mode==PLAN?sqlite3_mprintf("\"%w\"",t->aliases[i]):sqlite3_mprintf("b%d",i);
      if(t->mode==DIAMOND&&i==1)side=branch+1;
      if(i) {sqlite3_str_appendall(from," JOIN ");sqlite3_str_appendall(weight,"*");}
      if(mask&(1<<i)) {
        bits++;
        sqlite3_str_appendf(from,"(SELECT k,v,w FROM \"%w\".\"%w_delta\" WHERE side=%d AND w<>0) %s",t->schema,t->name,side,alias);
      } else if(t->source_view) sqlite3_str_appendf(from,"(SELECT k,v,1 AS w FROM \"%w\".\"%w_live_%d\") %s",t->schema,t->name,side,alias);
      else sqlite3_str_appendf(from,"(SELECT k,v,1 AS w FROM \"%w\".\"%w_state\" WHERE side=%d) %s",t->schema,t->name,side,alias);
      sqlite3_str_appendf(weight,"%s.w",alias);sqlite3_free(alias);
      if(i) {
        if(t->mode==PLAN)sqlite3_str_appendall(from," ON 1");
        else if(t->mode==SELF_CHAIN||t->mode==CHAIN||t->mode==DIAMOND)sqlite3_str_appendf(from," ON b%d.v=b%d.k",i-1,i);
        else sqlite3_str_appendf(from," ON b0.k=b%d.k",i);
      }
    }
    char *relations=sqlite3_str_finish(from),*w=sqlite3_str_finish(weight);
    if(t->mode==FANOUT){char *prior=w;w=sqlite3_mprintf("(%s*(coalesce(b0.v>=0,0)+coalesce(b0.v%%2=0,0)))",prior);sqlite3_free(prior);}
    const char *value=t->mode==PROJECT||t->mode==PLAN?t->projection:t->mode==SELF_CHAIN||t->mode==DIAMOND?"b1.v":t->mode==CHAIN?"b2.v":degree==3?"b0.v*b1.v*b2.v":degree==2?"b0.v*b1.v":"b0.v";
    const char *key_expr=t->mode==PLAN?t->key_expression:"b0.k";
    char *key=bag(t)?sqlite3_mprintf("json_array((%s),(%s))",key_expr,value):sqlite3_mprintf("json_array(b0.k)");
    char *group=bag(t)?sqlite3_mprintf("(%s),(%s)",key_expr,value):sqlite3_mprintf("b0.k");
    char *where=t->mode==PROJECT||t->mode==PLAN?sqlite3_mprintf("WHERE (%s)",t->predicate):sqlite3_mprintf("%s",t->mode==FILTER?"WHERE b0.v>=0":"");
    int sign=bits%2?1:-1;
    rc=sql(t,sqlite3_mprintf(
      "INSERT INTO \"%w\".\"%w_result\"(key,k,v,n,s,nn) SELECT %s,(%s),(%s),%d*sum(%s),%d*sum(coalesce((%s),0)*%s),%d*sum(((%s) IS NOT NULL)*%s) FROM %s %s GROUP BY %s "
      "ON CONFLICT(key) DO UPDATE SET n=n+excluded.n,s=s+excluded.s,nn=nn+excluded.nn",
      t->schema,t->name,key,key_expr,bag(t)?value:"NULL",
      sign,w,sign,value,w,sign,value,w,relations,where,group),0,0);
    sqlite3_free(key);sqlite3_free(group);sqlite3_free(where);
    sqlite3_free(relations);sqlite3_free(w);
  }
  if(rc==SQLITE_OK&&t->mode==PLAN) {
    sqlite3_int64 invalid=0;
    rc=scalar_sql(t,sqlite3_mprintf("SELECT count(*) FROM \"%w\".\"%w_result\" WHERE json_type(key,'$[0]') NOT IN ('integer','null') OR json_type(key,'$[1]') NOT IN ('integer','null')",t->schema,t->name),0,0,&invalid);
    if(rc==SQLITE_OK&&invalid)rc=error(t,"plan projections must produce integer or NULL before storage affinity");
  }
  if(rc==SQLITE_OK) {
    sqlite3_stmt *s=0;
    char *q=sqlite3_mprintf("SELECT count(*) FROM \"%w\".\"%w_result\" WHERE n<0 OR nn<0 OR nn>n OR typeof(n)<>'integer' OR typeof(s)<>'integer' OR typeof(nn)<>'integer' OR typeof(v) NOT IN ('integer','null') OR (n=0 AND (s<>0 OR nn<>0))",t->schema,t->name);
    rc=q?sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,0):SQLITE_NOMEM;sqlite3_free(q);
    if(rc==SQLITE_OK) {
      int step=sqlite3_step(s);
      if(step!=SQLITE_ROW) rc=step;
      else if(sqlite3_column_int(s,0))rc=error(t,"batch accumulator overflow or support invariant");
    }
    sqlite3_finalize(s);
  }
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DELETE FROM \"%w\".\"%w_result\" WHERE n=0",t->schema,t->name),0,0);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DELETE FROM \"%w\".\"%w_delta\"",t->schema,t->name),0,0);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_batch\" SET flag=0",t->schema,t->name),0,0);
  return rc;
}
static int source_view_setup(Tab *t) {
  if(!t->env->source_views||t->source_view)return SQLITE_OK;
  sqlite3_stmt *s=0;
  char *q=sqlite3_mprintf("SELECT side,source,id_col,k_col,v_col FROM \"%w\".\"%w_sources\" ORDER BY side",t->schema,t->name);
  int rc=q?sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,0):SQLITE_NOMEM;sqlite3_free(q);
  sqlite3_str *view=sqlite3_str_new(t->db);
  sqlite3_str_appendf(view,"CREATE VIEW \"%w\".\"%w_live\" AS ",t->schema,t->name);
  char *indexes[3]={0};
  char *views[3]={0};
  int count=0;
  while(rc==SQLITE_OK) {
    int step=sqlite3_step(s);if(step==SQLITE_DONE)break;if(step!=SQLITE_ROW){rc=step;break;}
    if(count>=3){rc=error(t,"too many source sides");break;}
    if(sqlite3_column_int(s,0)!=count){rc=error(t,"source sides must be contiguous from zero");break;}
    indexes[count]=sqlite3_mprintf("CREATE INDEX \"%w\".\"%w_live_key_%d\" ON \"%w\"(\"%w\")",t->schema,t->name,count,sqlite3_column_text(s,1),sqlite3_column_text(s,3));
    views[count]=sqlite3_mprintf("CREATE VIEW \"%w\".\"%w_live_%d\" AS SELECT \"%w\" AS id,\"%w\" AS k,\"%w\" AS v FROM \"%w\"",t->schema,t->name,count,sqlite3_column_text(s,2),sqlite3_column_text(s,3),sqlite3_column_text(s,4),sqlite3_column_text(s,1));
    if(count++)sqlite3_str_appendall(view," UNION ALL ");
    sqlite3_str_appendf(view,"SELECT \"%w\" AS id,\"%w\" AS k,\"%w\" AS v,%d AS side FROM \"%w\".\"%w\"",
      sqlite3_column_text(s,2),sqlite3_column_text(s,3),sqlite3_column_text(s,4),sqlite3_column_int(s,0),t->schema,sqlite3_column_text(s,1));
  }
  sqlite3_finalize(s);
  if(count!=sources(t)&&rc==SQLITE_OK)rc=error(t,"all source sides must be attached before source-view batching");
  q=sqlite3_str_finish(view);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DELETE FROM \"%w\".\"%w_state\"",t->schema,t->name),0,0);
  for(int i=0;i<3;i++){if(indexes[i]&&rc==SQLITE_OK)rc=sql(t,indexes[i],0,0);else sqlite3_free(indexes[i]);}
  for(int i=0;i<3;i++){if(views[i]&&rc==SQLITE_OK)rc=sql(t,views[i],0,0);else sqlite3_free(views[i]);}
  if(rc==SQLITE_OK)rc=sql(t,q,0,0);else sqlite3_free(q);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_batch\" SET source_views=1",t->schema,t->name),0,0);
  if(rc==SQLITE_OK)t->source_view=1;
  return rc;
}
static int batch_command(Tab *t,int op) {
  int flag=0,rc=batching(t,&flag);
  if(rc!=SQLITE_OK)return rc;
  if(!t->mode||sqlite3_get_autocommit(t->db))return error(t,"batch requires explicit transaction and a query mode");
  if(op==10) {
    if(flag)return error(t,"batch already open");
    if(t->frontiers&&t->epoch>=1000000000000)return error(t,"scalar epoch limit reached");
    rc=source_view_setup(t);
    if(rc==SQLITE_OK&&t->frontiers)rc=sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_clock\" SET epoch=epoch+1",t->schema,t->name),0,0);
    if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("UPDATE \"%w\".\"%w_batch\" SET flag=1",t->schema,t->name),0,0);
    return rc;
  }
  if(!flag)return error(t,"no open batch");
  if(t->frontiers){rc=frontier_check(t,-1,0);if(rc!=SQLITE_OK)return rc;}
  return flush_batch(t);
}
