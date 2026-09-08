/* SQLite parses and binds scalar SQL in a private plan-only connection.
 * The caller connection's authorizer and hooks are never changed. */
static int expression_auth(void *p,int op,const char *a,const char *b,const char *db,const char *trigger) {
  (void)db;(void)trigger;
  if(op==SQLITE_SELECT)return ++*(int *)p==1?SQLITE_OK:SQLITE_DENY;
  if(op==SQLITE_READ)return a&&!strcmp(a,"probe")&&b&&(!strcmp(b,"k")||!strcmp(b,"v")||!strcmp(b,""))?SQLITE_OK:SQLITE_DENY;
  if(op==SQLITE_FUNCTION)return b&&(!strcmp(b,"abs")||!strcmp(b,"coalesce")||!strcmp(b,"ifnull")||!strcmp(b,"nullif"))?SQLITE_OK:SQLITE_DENY;
  return SQLITE_DENY;
}
static char *literal(const char *s) {
  size_t n=strlen(s);
  if(n<2||n>2048||s[0]!='\''||s[n-1]!='\'')return 0;
  char *out=sqlite3_malloc64(n);if(!out)return 0;
  size_t j=0;
  for(size_t i=1;i<n-1;i++) {
    if(s[i]=='\'') {if(i+1>=n-1||s[i+1]!='\''){sqlite3_free(out);return 0;}i++;}
    out[j++]=s[i];
  }
  out[j]=0;return out;
}
static int validate_expressions(Tab *t) {
  sqlite3 *plan=0;sqlite3_stmt *s=0;int count=0;
  int rc=sqlite3_open(":memory:",&plan);
  if(rc==SQLITE_OK)rc=sqlite3_exec(plan,"CREATE TABLE probe(k INTEGER,v INTEGER)",0,0,0);
  if(rc==SQLITE_OK)rc=sqlite3_set_authorizer(plan,expression_auth,&count);
  char *q=sqlite3_mprintf("SELECT (%s),(%s) FROM probe b0",t->predicate,t->projection);
  const char *tail=0;
  if(rc==SQLITE_OK)rc=q?sqlite3_prepare_v3(plan,q,-1,SQLITE_PREPARE_NO_VTAB,&s,&tail):SQLITE_NOMEM;
  if(rc==SQLITE_OK&&(!tail||*tail||sqlite3_bind_parameter_count(s)||sqlite3_column_count(s)!=2))rc=SQLITE_ERROR;
  sqlite3_finalize(s);sqlite3_free(q);sqlite3_close(plan);return rc;
}
