"""Reuse Take 2 SQL transport with an explicit batch boundary per mutation."""
import importlib.util
from pathlib import Path
import sqlite3
import sys

p=Path(__file__).resolve().parent.parent/'42_sqlite_native_take2/7_shootout.py'
spec=importlib.util.spec_from_file_location('transport',p)
transport=importlib.util.module_from_spec(spec);spec.loader.exec_module(transport)
source_views='--source-views' in sys.argv
if source_views:sys.argv.remove('--source-views')
lazy='--lazy' in sys.argv
if lazy:sys.argv.remove('--lazy')

class BatchConnection(sqlite3.Connection):
    opened=False
    def executescript(self,sql):
        return super().executescript(sql.replace('USING take2(join)','USING take2_lazy(join)') if lazy else sql)
    def execute(self,sql,parameters=()):
        if lazy:sql=sql.replace('USING take2(join)','USING take2_lazy(join)')
        if not lazy and sql=='COMMIT' and self.opened:
            super().execute('INSERT INTO native_result(op) VALUES(11)')
        cursor=super().execute(sql,parameters)
        if not lazy and sql=='BEGIN IMMEDIATE' and self.opened:
            super().execute('INSERT INTO native_result(op) VALUES(10)')
        if sql=="SELECT take2_control('trace')":self.opened=True
        if source_views and 'take2_attach' in sql and "'dimension'" in sql:self.opened=True
        return cursor

def connection(path,extension):
    db=sqlite3.connect(path,isolation_level=None,factory=BatchConnection)
    db.enable_load_extension(True);db.load_extension(extension);db.enable_load_extension(False)
    db.execute('PRAGMA recursive_triggers=ON');db.execute('PRAGMA cache_size=-8192')
    db.execute("SELECT take2_control('cache_on')")
    if source_views:db.execute("SELECT take2_control('source_views_on')")
    return db

transport.connection=connection
original_emit=transport.emit
def emit(**row):
    if row.get('event')=='case-setup':
        row['algorithm']='explicit SQL batch; consolidated signed deltas; 32 cached statements'
        row['consistency']='maintained reads fail inside open batch; flush before COMMIT; source and output commit atomically'
        row['source_images']='indexed source views' if source_views else 'shadow copies'
        if lazy:
            row['algorithm']='public vtab lazy read/xSync flush; consolidated signed deltas'
            row['consistency']='completed-statement reads flush pending deltas; xSync prepares atomic source/output commit'
    original_emit(**row)
transport.emit=emit
if __name__=='__main__':transport.main()
