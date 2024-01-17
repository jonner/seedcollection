-- Add migration script here
ALTER TABLE sc_locations RENAME COLUMN locid TO srcid;
ALTER TABLE sc_locations RENAME COLUMN name TO srcname;
ALTER TABLE sc_locations RENAME COLUMN description TO srcdesc;
ALTER TABLE sc_locations RENAME TO sc_sources;
ALTER TABLE sc_samples RENAME COLUMN collectedlocation TO srcid;
