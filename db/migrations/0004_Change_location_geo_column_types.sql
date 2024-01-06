PRAGMA defer_foreign_keys=on;
CREATE TABLE "sc_locations_new" (
	"locid"	INTEGER NOT NULL,
	"name"	TEXT NOT NULL,
	"description"	TEXT,
	"latitude"	REAL,
	"longitude"	REAL,
	PRIMARY KEY("locid" AUTOINCREMENT)
);
INSERT INTO sc_locations_new (locid, name, description, latitude, longitude) SELECT locid, name, description, latitude, longitude FROM sc_locations;
DROP TABLE sc_locations;
ALTER TABLE sc_locations_new RENAME to sc_locations;
PRAGMA defer_foreign_keys=off;
