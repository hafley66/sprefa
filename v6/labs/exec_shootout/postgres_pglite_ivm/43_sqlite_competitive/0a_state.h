typedef struct Env {
  char trace[16384];
  int used, events, logging, fail_sync;
  int cache;
  int source_views;
  int references;
  sqlite3_int64 prepares,steps,vm,scans,prepare_ns,step_ns;
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
  sqlite3_int64 epoch;
  char *predicate,*projection;
} Tab;
typedef struct Cursor {
  sqlite3_vtab_cursor base;
  sqlite3_stmt *stmt;
  int eof;
  sqlite3_int64 repeats,ordinal;
} Cursor;
enum { MIRROR, FILTER, BAG, GROUP, JOIN, SELF, MULTI, PROJECT, INNER, SELF_CHAIN, CHAIN, SEMI, ANTI, REACH, DISTINCT, FANOUT, DIAMOND };
static int arity(Tab *t) {return t->mode==MULTI||t->mode==CHAIN?3:t->mode==JOIN||t->mode==SELF||t->mode==INNER||t->mode==SELF_CHAIN||t->mode==DIAMOND?2:1;}
static int sources(Tab *t) {return t->mode==MULTI||t->mode==CHAIN||t->mode==DIAMOND?3:t->mode==JOIN||t->mode==INNER||t->mode==SEMI||t->mode==ANTI||t->mode==REACH?2:1;}
static int bag(Tab *t) {return t->mode==FILTER||t->mode==BAG||(t->mode>=PROJECT&&t->mode!=REACH);}
static int sql(Tab *, char *, sqlite3_value **, int);
static int error(Tab *, const char *);
