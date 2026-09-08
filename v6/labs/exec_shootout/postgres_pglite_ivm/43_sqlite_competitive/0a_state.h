typedef struct Env {
  char trace[16384];
  int used, events, logging, fail_sync;
  int cache;
  int source_views;
  int preparing;
  int references;
  sqlite3_int64 prepares,steps,vm,scans,prepare_ns,step_ns;
  sqlite3_int64 delta_build_ns,reprepares,scalar_steps;
  sqlite3_int64 batch_read_ns,pragma_ns,enqueue_ns,stats_ns,flush_ns;
} Env;
typedef struct Cached { char *text; sqlite3_stmt *stmt; } Cached;
typedef struct Tab {
  sqlite3_vtab base;
  sqlite3 *db;
  Env *env;
  char *schema, *name;
  int busy, mode;
  Cached cached[32];
  int next_cache;
  sqlite3_stmt *batch_read;
  int source_view;
  int frontiers;
  int lazy;
  int fused;
  sqlite3_int64 epoch;
  char *predicate,*projection;
  int degree,source_count,side_map[3];
  char *aliases[3],*key_expression;
} Tab;
typedef struct Cursor {
  sqlite3_vtab_cursor base;
  sqlite3_stmt *stmt;
  int eof;
  sqlite3_int64 repeats,ordinal;
} Cursor;
enum { MIRROR, FILTER, BAG, GROUP, JOIN, SELF, MULTI, PROJECT, INNER, SELF_CHAIN, CHAIN, SEMI, ANTI, REACH, DISTINCT, FANOUT, DIAMOND, PLAN };
static int arity(Tab *t) {return t->mode==PLAN?t->degree:t->mode==MULTI||t->mode==CHAIN?3:t->mode==JOIN||t->mode==SELF||t->mode==INNER||t->mode==SELF_CHAIN||t->mode==DIAMOND?2:1;}
static int sources(Tab *t) {return t->mode==PLAN?t->source_count:t->mode==MULTI||t->mode==CHAIN||t->mode==DIAMOND?3:t->mode==JOIN||t->mode==INNER||t->mode==SEMI||t->mode==ANTI||t->mode==REACH?2:1;}
static int bag(Tab *t) {return t->mode==FILTER||t->mode==BAG||(t->mode>=PROJECT&&t->mode!=REACH);}
static int sql(Tab *, char *, sqlite3_value **, int);
static int scalar_sql(Tab *, char *, sqlite3_value **, int,sqlite3_int64 *);
static int error(Tab *, const char *);
static sqlite3_int64 now_ns(void){struct timespec stamp;clock_gettime(CLOCK_MONOTONIC,&stamp);return (sqlite3_int64)stamp.tv_sec*1000000000+stamp.tv_nsec;}
