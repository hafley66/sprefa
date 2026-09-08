/* Bounded public session API mechanism probe, linked to task-owned SQLite.
 * This executable owns its connection and the session's preupdate hook.
 * It does not register or replace a preupdate hook itself. */
#define SQLITE_ENABLE_SESSION 1
#include "sqlite3.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Capture {sqlite3 *db;sqlite3_session *session;int drains,changes;sqlite3_int64 peak;} Capture;
static void require(int condition,const char *label){if(!condition){fprintf(stderr,"probe assertion: %s\n",label);exit(1);}}
static void run(Capture *c,const char *sql){char *error=0;int rc=sqlite3_exec(c->db,sql,0,0,&error);if(rc){fprintf(stderr,"SQL failed rc=%d: %s\n",rc,error?error:"");sqlite3_free(error);exit(1);}}
static void reset(Capture *c){if(c->session)sqlite3session_delete(c->session);c->session=0;require(sqlite3session_create(c->db,"main",&c->session)==SQLITE_OK,"session create");require(sqlite3session_attach(c->session,"a")==SQLITE_OK,"session attach");}
static int same(Capture *c){sqlite3_stmt *s=0;require(sqlite3_prepare_v2(c->db,"SELECT NOT EXISTS(SELECT * FROM a EXCEPT SELECT * FROM maintained) AND NOT EXISTS(SELECT * FROM maintained EXCEPT SELECT * FROM a)",-1,&s,0)==SQLITE_OK,"oracle prepare");require(sqlite3_step(s)==SQLITE_ROW,"oracle step");int equal=sqlite3_column_int(s,0);sqlite3_finalize(s);return equal;}
static int drain(Capture *c){
  sqlite3_int64 bytes=sqlite3session_memory_used(c->session);
  if(bytes>c->peak)c->peak=bytes;
  if(bytes>65536)return SQLITE_TOOBIG;
  void *data=0;int size=0,rc=sqlite3session_changeset(c->session,&size,&data);
  if(rc!=SQLITE_OK)return rc;
  if(size>65536){sqlite3_free(data);return SQLITE_TOOBIG;}
  sqlite3_changeset_iter *it=0;
  rc=sqlite3changeset_start(&it,size,data);
  while(rc==SQLITE_OK){
    int step=sqlite3changeset_next(it);if(step==SQLITE_DONE)break;if(step!=SQLITE_ROW){rc=step;break;}
    const char *table=0;int columns=0,op=0,indirect=0;
    rc=sqlite3changeset_op(it,&table,&columns,&op,&indirect);
    if(rc!=SQLITE_OK)break;
    if(strcmp(table,"a")||columns!=3){rc=SQLITE_ERROR;break;}
    sqlite3_value *id=0;
    rc=op==SQLITE_INSERT?sqlite3changeset_new(it,0,&id):sqlite3changeset_old(it,0,&id);
    if(rc!=SQLITE_OK||!id||sqlite3_value_type(id)!=SQLITE_INTEGER){rc=SQLITE_ERROR;break;}
    sqlite3_stmt *s=0;
    rc=sqlite3_prepare_v2(c->db,op==SQLITE_DELETE?"DELETE FROM maintained WHERE id=?1":"INSERT INTO maintained SELECT * FROM a WHERE id=?1 ON CONFLICT(id) DO UPDATE SET k=excluded.k,v=excluded.v",-1,&s,0);
    if(rc==SQLITE_OK)rc=sqlite3_bind_value(s,1,id);
    if(rc==SQLITE_OK){int result=sqlite3_step(s);rc=result==SQLITE_DONE?SQLITE_OK:result;}
    sqlite3_finalize(s);c->changes++;
  }
  sqlite3changeset_finalize(it);sqlite3_free(data);
  if(rc==SQLITE_OK){reset(c);c->drains++;}
  return rc;
}
static void sql_drain(sqlite3_context *ctx,int argc,sqlite3_value **argv){(void)argc;(void)argv;int rc=drain(sqlite3_user_data(ctx));if(rc!=SQLITE_OK)sqlite3_result_error_code(ctx,rc);}
static void clean(Capture *c){run(c,"DELETE FROM a;DELETE FROM maintained");reset(c);}
int main(int argc,char **argv){
  require(argc==2,"task-owned fresh database path required");
  Capture c={0};require(sqlite3_open(argv[1],&c.db)==SQLITE_OK,"open");
  run(&c,"PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL;PRAGMA recursive_triggers=ON;CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);CREATE TABLE maintained(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)");
  reset(&c);
  require(sqlite3_create_function_v2(c.db,"capture_drain",0,SQLITE_UTF8|SQLITE_DIRECTONLY,&c,sql_drain,0,0,0)==SQLITE_OK,"register drain");
  /* Naive reset loses the prefix whose maintenance rolled back with s. */
  run(&c,"BEGIN;INSERT INTO a VALUES(1,1,2);SAVEPOINT s;INSERT INTO a VALUES(2,2,3);SELECT capture_drain();ROLLBACK TO s;RELEASE s;SELECT capture_drain()");
  require(!same(&c),"naive reset must expose rollback gap");
  puts("{\"case\":\"naive_reset_after_drain\",\"expected_gap_observed\":true}");
  run(&c,"ROLLBACK");clean(&c);
  /* Flush pending prefix before every savepoint, reset after rollback-to. */
  run(&c,"BEGIN;INSERT INTO a VALUES(1,1,2);SELECT capture_drain();SAVEPOINT s;INSERT INTO a VALUES(2,2,3);SELECT capture_drain();SAVEPOINT nested;UPDATE a SET v=v+1;SELECT capture_drain();RELEASE nested;ROLLBACK TO s");
  reset(&c);require(same(&c),"prefix maintenance restored at rollback-to");
  run(&c,"RELEASE s;INSERT INTO a VALUES(3,3,NULL);SELECT capture_drain();COMMIT");
  require(same(&c),"commit oracle");puts("{\"case\":\"flush_before_savepoint_reset_after_rollback\",\"exact\":true}");
  run(&c,"BEGIN;UPDATE a SET v=9;DELETE FROM a WHERE id=1;SELECT capture_drain();ROLLBACK");reset(&c);require(same(&c),"outer rollback");
  for(int i=0;i<4;i++){
    clean(&c);run(&c,"BEGIN");
    const char *conflicts[]={"ABORT","FAIL","IGNORE","REPLACE"};
    char *q=sqlite3_mprintf("INSERT OR %s INTO a VALUES(1,1,2),(1,3,4),(2,5,6)",conflicts[i]);
    int rc=sqlite3_exec(c.db,q,0,0,0);sqlite3_free(q);
    require(rc==(i<2?SQLITE_CONSTRAINT:SQLITE_OK),"conflict contract");
    run(&c,"SELECT capture_drain();COMMIT");require(same(&c),"conflict oracle");
    printf("{\"case\":\"%s\",\"exact\":true}\n",conflicts[i]);
  }
  run(&c,"BEGIN;UPDATE a SET id=id+10;SELECT capture_drain();COMMIT");require(same(&c),"primary-key changes");
  run(&c,"BEGIN;INSERT INTO a VALUES(50,1,1);UPDATE a SET v=2 WHERE id=50;DELETE FROM a WHERE id=50;SELECT capture_drain();COMMIT");require(same(&c),"net cancellation");
  run(&c,"CREATE TRIGGER misuse BEFORE INSERT ON a BEGIN SELECT capture_drain();END");
  require(sqlite3_exec(c.db,"INSERT INTO a VALUES(99,1,1)",0,0,0)!=SQLITE_OK,"direct-only misuse");
  run(&c,"DROP TRIGGER misuse");
  sqlite3session_delete(c.session);c.session=0;require(sqlite3_close(c.db)==SQLITE_OK,"close");
  require(sqlite3_open(argv[1],&c.db)==SQLITE_OK,"reopen");require(same(&c),"durable reopen");sqlite3_close(c.db);
  printf("{\"case\":\"summary\",\"drains\":%d,\"changed_rows\":%d,\"peak_session_bytes\":%lld,\"bounded_bytes\":65536,\"reopen_exact\":true,\"production_scheduler\":false}\n",c.drains,c.changes,(long long)c.peak);
  return 0;
}
