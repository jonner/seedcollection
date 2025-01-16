-- Add migration script here
-- History tables created with sqlite_history: https://github.com/simonw/sqlite-history
CREATE TABLE IF NOT EXISTS _sc_samples_history (
    _rowid INTEGER,
   sampleid INTEGER,
   tsn INTEGER,
   certainty INTEGER,
   month INTEGER,
   year INTEGER,
   srcid TEXT,
   notes TEXT,
   quantity REAL,
   userid INTEGER,
    _version INTEGER,
    _updated INTEGER,
    _mask INTEGER
);
DROP TRIGGER IF EXISTS sc_samples_insert_history;
CREATE TRIGGER sc_samples_insert_history
AFTER INSERT ON sc_samples
BEGIN
    INSERT INTO _sc_samples_history (_rowid, sampleid, tsn, certainty, month, year, srcid, notes, quantity, userid, _version, _updated, _mask)
    VALUES (new.rowid, new.sampleid, new.tsn, new.certainty, new.month, new.year, new.srcid, new.notes, new.quantity, new.userid, 1, cast((julianday('now') - 2440587.5) * 86400 * 1000 as integer), 511);
END;
DROP TRIGGER IF EXISTS sc_samples_update_history;
CREATE TRIGGER sc_samples_update_history
AFTER UPDATE ON sc_samples
FOR EACH ROW
BEGIN
    INSERT INTO _sc_samples_history (_rowid, sampleid, tsn, certainty, month, year, srcid, notes, quantity, userid, _version, _updated, _mask)
    SELECT old.rowid, 
        CASE WHEN old.sampleid IS NOT new.sampleid then new.sampleid else null end, 
        CASE WHEN old.tsn IS NOT new.tsn then new.tsn else null end, 
        CASE WHEN old.certainty IS NOT new.certainty then new.certainty else null end, 
        CASE WHEN old.month IS NOT new.month then new.month else null end, 
        CASE WHEN old.year IS NOT new.year then new.year else null end, 
        CASE WHEN old.srcid IS NOT new.srcid then new.srcid else null end, 
        CASE WHEN old.notes IS NOT new.notes then new.notes else null end, 
        CASE WHEN old.quantity IS NOT new.quantity then new.quantity else null end, 
        CASE WHEN old.userid IS NOT new.userid then new.userid else null end,
        (SELECT MAX(_version) FROM _sc_samples_history WHERE _rowid = old.rowid) + 1,
        cast((julianday('now') - 2440587.5) * 86400 * 1000 as integer),
        (CASE WHEN old.sampleid IS NOT new.sampleid then 1 else 0 end) + (CASE WHEN old.tsn IS NOT new.tsn then 2 else 0 end) + (CASE WHEN old.certainty IS NOT new.certainty then 4 else 0 end) + (CASE WHEN old.month IS NOT new.month then 8 else 0 end) + (CASE WHEN old.year IS NOT new.year then 16 else 0 end) + (CASE WHEN old.srcid IS NOT new.srcid then 32 else 0 end) + (CASE WHEN old.notes IS NOT new.notes then 64 else 0 end) + (CASE WHEN old.quantity IS NOT new.quantity then 128 else 0 end) + (CASE WHEN old.userid IS NOT new.userid then 256 else 0 end)
    WHERE old.sampleid IS NOT new.sampleid or old.tsn IS NOT new.tsn or old.certainty IS NOT new.certainty or old.month IS NOT new.month or old.year IS NOT new.year or old.srcid IS NOT new.srcid or old.notes IS NOT new.notes or old.quantity IS NOT new.quantity or old.userid IS NOT new.userid;
END;
DROP TRIGGER IF EXISTS sc_samples_delete_history;
CREATE TRIGGER sc_samples_delete_history
AFTER DELETE ON sc_samples
BEGIN
    INSERT INTO _sc_samples_history (_rowid, sampleid, tsn, certainty, month, year, srcid, notes, quantity, userid, _version, _updated, _mask)
    VALUES (
        old.rowid,
        old.sampleid, old.tsn, old.certainty, old.month, old.year, old.srcid, old.notes, old.quantity, old.userid,
        (SELECT COALESCE(MAX(_version), 0) from _sc_samples_history WHERE _rowid = old.rowid) + 1,
        cast((julianday('now') - 2440587.5) * 86400 * 1000 as integer),
        -1
    );
END;
