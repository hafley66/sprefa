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
  if(n<2||n>8194||s[0]!='\''||s[n-1]!='\'')return 0;
  char *out=sqlite3_malloc64(n);if(!out)return 0;
  size_t j=0;
  for(size_t i=1;i<n-1;i++) {
    if(s[i]=='\'') {if(i+1>=n-1||s[i+1]!='\''){sqlite3_free(out);return 0;}i++;}
    out[j++]=s[i];
  }
  out[j]=0;return out;
}
/* JSON decoding and expression binding use SQLite, on a private connection.
 * Persisted plan SQL is revalidated at every xConnect, including reopen. */
static int load_plan(Tab *t,const char *argument) {
  char *json=literal(argument);if(!json||strlen(json)>4096){sqlite3_free(json);return SQLITE_ERROR;}
  sqlite3 *db=0;sqlite3_stmt *s=0;int rc=sqlite3_open(":memory:",&db);
  const char *query="SELECT json_extract(?1,'$.version'),json_array_length(?1,'$.sides'),json_array_length(?1,'$.aliases'),json_extract(?1,'$.key'),json_extract(?1,'$.value'),json_extract(?1,'$.predicate'),json_extract(?1,'$.aggregate')";
  if(rc==SQLITE_OK)rc=sqlite3_prepare_v2(db,query,-1,&s,0);
  if(rc==SQLITE_OK)rc=sqlite3_bind_text(s,1,json,-1,SQLITE_STATIC);
  if(rc==SQLITE_OK) {
    if(sqlite3_step(s)!=SQLITE_ROW||sqlite3_column_type(s,0)!=SQLITE_INTEGER||sqlite3_column_int64(s,0)!=1||sqlite3_column_int(s,1)<1||sqlite3_column_int(s,1)>3||sqlite3_column_int(s,1)!=sqlite3_column_int(s,2))rc=SQLITE_ERROR;
    for(int i=3;i<6&&rc==SQLITE_OK;i++)if(sqlite3_column_type(s,i)!=SQLITE_TEXT||memchr(sqlite3_column_text(s,i),0,(size_t)sqlite3_column_bytes(s,i)))rc=SQLITE_ERROR;
    if(rc==SQLITE_OK&&sqlite3_column_type(s,6)!=SQLITE_NULL){if(sqlite3_column_type(s,6)!=SQLITE_INTEGER||sqlite3_column_int64(s,6)<0||sqlite3_column_int64(s,6)>1)rc=SQLITE_ERROR;else t->plan_group=sqlite3_column_int(s,6);}
    if(rc==SQLITE_OK){t->degree=sqlite3_column_int(s,1);t->key_expression=sqlite3_mprintf("%s",sqlite3_column_text(s,3));t->projection=sqlite3_mprintf("%s",sqlite3_column_text(s,4));t->predicate=sqlite3_mprintf("%s",sqlite3_column_text(s,5));if(!t->key_expression||!t->projection||!t->predicate)rc=SQLITE_NOMEM;}
  }
  sqlite3_finalize(s);s=0;
  if(rc==SQLITE_OK)rc=sqlite3_prepare_v2(db,"SELECT json_extract(?1,'$.sides['||?2||']'),json_extract(?1,'$.aliases['||?2||']')",-1,&s,0);
  for(int i=0;i<t->degree&&rc==SQLITE_OK;i++) {
    sqlite3_bind_text(s,1,json,-1,SQLITE_STATIC);sqlite3_bind_int(s,2,i);
    if(sqlite3_step(s)!=SQLITE_ROW||sqlite3_column_type(s,0)!=SQLITE_INTEGER||sqlite3_column_type(s,1)!=SQLITE_TEXT)rc=SQLITE_ERROR;
    else {
      t->side_map[i]=sqlite3_column_int(s,0);
      if(sqlite3_column_int64(s,0)<0||sqlite3_column_int64(s,0)>t->source_count)rc=SQLITE_ERROR;
      if(t->side_map[i]==t->source_count)t->source_count++;
      const char *alias=(const char *)sqlite3_column_text(s,1);int bytes=sqlite3_column_bytes(s,1);
      if(bytes<1||bytes>128||memchr(alias,0,(size_t)bytes))rc=SQLITE_ERROR;
      t->aliases[i]=sqlite3_mprintf("%s",alias);
      if(!t->aliases[i])rc=SQLITE_NOMEM;
      for(int j=0;j<i&&rc==SQLITE_OK;j++)if(!sqlite3_stricmp(t->aliases[j],t->aliases[i]))rc=SQLITE_ERROR;
    }
    sqlite3_reset(s);
  }
  sqlite3_finalize(s);s=0;sqlite3_free(json);
  if(rc==SQLITE_OK)rc=sqlite3_exec(db,"CREATE TABLE probe(k INTEGER,v INTEGER)",0,0,0);
  int selects=0;
  if(rc==SQLITE_OK)rc=sqlite3_set_authorizer(db,expression_auth,&selects);
  if(rc==SQLITE_OK) {
    sqlite3_str *builder=sqlite3_str_new(db);
    sqlite3_str_appendf(builder,"SELECT (%s),(%s) FROM ",t->key_expression,t->projection);
    for(int i=0;i<t->degree;i++)sqlite3_str_appendf(builder,"%sprobe AS \"%w\"",i?",":"",t->aliases[i]);
    sqlite3_str_appendf(builder," WHERE (%s)",t->predicate);
    char *q=sqlite3_str_finish(builder);const char *tail=0;
    rc=q?sqlite3_prepare_v3(db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,&tail):SQLITE_NOMEM;
    if(rc==SQLITE_OK&&(*tail||sqlite3_bind_parameter_count(s)||sqlite3_column_count(s)!=2))rc=SQLITE_ERROR;
    sqlite3_finalize(s);sqlite3_free(q);
  }
  sqlite3_close(db);return rc;
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
/* Check the expression's SQL type before INTEGER storage affinity can coerce
 * numeric text or real values and silently change projection semantics. */
static int projection_domain(Tab *t) {
  sqlite3_stmt *s=0;
  char *q=sqlite3_mprintf("SELECT count(*) FROM \"%w\".\"%w_delta\" b0 WHERE side=0 AND w<>0 AND (%s) AND typeof(%s) NOT IN ('integer','null')",t->schema,t->name,t->predicate,t->projection);
  int rc=q?sqlite3_prepare_v3(t->db,q,-1,SQLITE_PREPARE_NO_VTAB,&s,0):SQLITE_NOMEM;sqlite3_free(q);
  if(rc==SQLITE_OK){int step=sqlite3_step(s);if(step!=SQLITE_ROW)rc=step;else if(sqlite3_column_int(s,0))rc=error(t,"projection must produce integer or NULL before storage affinity");}
  sqlite3_finalize(s);return rc;
}
