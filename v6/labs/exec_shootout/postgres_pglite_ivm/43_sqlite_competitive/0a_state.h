typedef struct Env {
  char trace[16384];
  int used, events, logging, fail_sync;
  int cache;
  int source_views;
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
} Tab;
typedef struct Cursor {
  sqlite3_vtab_cursor base;
  sqlite3_stmt *stmt;
  int eof;
} Cursor;
enum { MIRROR, FILTER, BAG, GROUP, JOIN, SELF, MULTI };
static int sql(Tab *, char *, sqlite3_value **, int);
static int error(Tab *, const char *);
