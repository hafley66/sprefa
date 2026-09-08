typedef struct Env {
  char trace[16384];
  int used, events, logging, fail_sync;
} Env;
typedef struct Tab {
  sqlite3_vtab base;
  sqlite3 *db;
  Env *env;
  char *schema, *name;
  int busy, mode;
} Tab;
typedef struct Cursor {
  sqlite3_vtab_cursor base;
  sqlite3_stmt *stmt;
  int eof;
} Cursor;
enum { MIRROR, FILTER, BAG, GROUP, JOIN, SELF, MULTI };
static int sql(Tab *, char *, sqlite3_value **, int);
static int error(Tab *, const char *);
