/* Current SQLite source images, used only from the explicit flush statement. */
static char *relation(Tab *t,int side) {
  char *base=t->source_view?sqlite3_mprintf("(SELECT k,v FROM \"%w\".\"%w_live_%d\")",t->schema,t->name,side):
    sqlite3_mprintf("(SELECT k,v FROM \"%w\".\"%w_state\" WHERE side=%d)",t->schema,t->name,side);
  if(t->mode==REACH&&t->predicate){char *filtered=sqlite3_mprintf("(SELECT k,v FROM %s b0 WHERE (%s))",base,side?t->projection:t->predicate);sqlite3_free(base);return filtered;}
  return base;
}

/* Telescoping: dLeft * oldMembership + currentLeft * dMembership.
 * Right duplicates change membership only when their support crosses zero. */
static int flush_membership(Tab *t) {
  char *left=relation(t,0),*right=relation(t,1);
  const char *cmp=t->mode==ANTI?"=0":">0";
  char *q=sqlite3_mprintf(
    "WITH d AS MATERIALIZED (SELECT k,v,w FROM \"%w\".\"%w_delta\" WHERE side=0 AND w<>0),"
    "r AS MATERIALIZED (SELECT k,sum(w) AS w FROM \"%w\".\"%w_delta\" WHERE side=1 GROUP BY k HAVING sum(w)<>0),"
    "changed AS MATERIALIZED (SELECT k,w,(SELECT count(*) FROM %s b WHERE b.k=r.k) AS n FROM r),"
    "terms AS (SELECT d.k,d.v,d.w*((SELECT count(*) FROM %s b WHERE b.k=d.k)-coalesce((SELECT w FROM r WHERE r.k=d.k),0)%s) AS w FROM d "
    "UNION ALL SELECT a.k,a.v,((r.n%s)-(r.n-r.w%s)) FROM %s a JOIN changed r ON a.k=r.k) "
    "INSERT INTO \"%w\".\"%w_result\"(key,k,v,n,s,nn) SELECT json_array(k,v),k,v,sum(w),sum(coalesce(v,0)*w),sum((v IS NOT NULL)*w) FROM terms GROUP BY k,v "
    "ON CONFLICT(key) DO UPDATE SET n=n+excluded.n,s=s+excluded.s,nn=nn+excluded.nn",
    t->schema,t->name,t->schema,t->name,right,right,cmp,cmp,cmp,left,t->schema,t->name);
  sqlite3_free(left);sqlite3_free(right);
  return sql(t,q,0,0);
}

/* DRed set reachability. Overdelete only the forward cone of removed supports;
 * rederive from surviving incoming edges/roots, then propagate new supports.
 * UNION deduplicates cycles. SQLite owns cone and result across savepoints. */
static int flush_reach(Tab *t) {
  char *edges=relation(t,0),*roots=relation(t,1);
  char *result=sqlite3_mprintf("\"%w\".\"%w_result\"",t->schema,t->name);
  char *delta=sqlite3_mprintf("\"%w\".\"%w_delta\"",t->schema,t->name);
  if(t->predicate){char *filtered=sqlite3_mprintf("(SELECT side,k,v,w FROM %s b0 WHERE CASE side WHEN 0 THEN (%s) ELSE (%s) END)",delta,t->predicate,t->projection);sqlite3_free(delta);delta=filtered;}
  char *cone=sqlite3_mprintf("\"%w\".\"%w_cone\"",t->schema,t->name);
  int rc=sql(t,sqlite3_mprintf(
    "WITH RECURSIVE affected(k) AS MATERIALIZED ("
    "SELECT CASE side WHEN 0 THEN v ELSE k END FROM %s WHERE w<0 "
    "UNION SELECT e.v FROM %s e JOIN affected a ON e.k=a.k JOIN %s r ON r.k=e.v) "
    "INSERT OR IGNORE INTO %s SELECT a.k FROM affected a JOIN %s r ON r.k=a.k",
    delta,edges,result,cone,result),0,0);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DELETE FROM %s WHERE k IN (SELECT k FROM %s)",result,cone),0,0);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf(
    "WITH RECURSIVE seed(k) AS ("
    "SELECT c.k FROM %s c WHERE EXISTS(SELECT 1 FROM %s b WHERE b.k=c.k) "
    "OR EXISTS(SELECT 1 FROM %s e JOIN %s r ON e.k=r.k WHERE e.v=c.k) "
    "UNION SELECT k FROM %s WHERE side=1 AND w>0 "
    "UNION SELECT d.v FROM %s d JOIN %s r ON d.k=r.k WHERE d.side=0 AND d.w>0),"
    "added(k) AS MATERIALIZED (SELECT k FROM seed WHERE NOT EXISTS(SELECT 1 FROM %s r WHERE r.k=seed.k) "
    "UNION SELECT e.v FROM %s e JOIN added a ON e.k=a.k WHERE NOT EXISTS(SELECT 1 FROM %s r WHERE r.k=e.v)) "
    "INSERT INTO %s(key,k,v,n,s,nn) SELECT json_array(k),k,NULL,1,0,0 FROM added WHERE true ON CONFLICT(key) DO NOTHING",
    cone,roots,edges,result,delta,delta,result,result,edges,result,result),0,0);
  if(rc==SQLITE_OK)rc=sql(t,sqlite3_mprintf("DELETE FROM %s",cone),0,0);
  sqlite3_free(edges);sqlite3_free(roots);sqlite3_free(result);sqlite3_free(delta);sqlite3_free(cone);
  return rc;
}
