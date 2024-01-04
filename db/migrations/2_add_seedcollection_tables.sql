CREATE TABLE IF NOT EXISTS "germinationcodes" (
	"id"	INTEGER NOT NULL UNIQUE,
	"code"	TEXT NOT NULL,
	"summary"	TEXT,
	"description"	TEXT,
	PRIMARY KEY("id" AUTOINCREMENT)
);
CREATE TABLE IF NOT EXISTS "mntaxa" (
	"id"	INTEGER UNIQUE,
	"tsn"	INTEGER UNIQUE,
	"native_status"	TEXT,
	PRIMARY KEY("id" AUTOINCREMENT),
	FOREIGN KEY("tsn") REFERENCES "taxonomic_units"("tsn")
);
CREATE TABLE IF NOT EXISTS "seedcollections" (
	"id"	INTEGER NOT NULL,
	"name"	TEXT NOT NULL,
	"description"	TEXT,
	PRIMARY KEY("id" AUTOINCREMENT)
);
CREATE TABLE IF NOT EXISTS "seedcollectionsamples" (
	"id"	INTEGER,
	"collectionid"	INTEGER NOT NULL,
	"sampleid"	INTEGER NOT NULL,
	PRIMARY KEY("id" AUTOINCREMENT),
	FOREIGN KEY("collectionid") REFERENCES "seedcollections"("id"),
	FOREIGN KEY("sampleid") REFERENCES "seedsamples"("id"),
	UNIQUE("collectionid","sampleid")
);
CREATE TABLE IF NOT EXISTS "seedlocations" (
	"locid"	INTEGER NOT NULL,
	"name"	TEXT NOT NULL,
	"description"	TEXT,
	"latitude"	NUMERIC,
	"longitude"	NUMERIC,
	PRIMARY KEY("locid" AUTOINCREMENT)
);
CREATE TABLE IF NOT EXISTS "seedsamples" (
	"id"	INTEGER NOT NULL,
	"tsn"	INTEGER NOT NULL,
	"certainty"	INTEGER,
	"month"	INTEGER,
	"year"	INTEGER,
	"collectedlocation"	TEXT,
	"notes"	TEXT,
	"quantity"	INTEGER,
	PRIMARY KEY("id" AUTOINCREMENT),
	FOREIGN KEY("collectedlocation") REFERENCES "seedlocations"("locid")
);
CREATE TABLE IF NOT EXISTS "taxongermination" (
	"id"	INTEGER NOT NULL UNIQUE,
	"tsn"	INTEGER NOT NULL,
	"germid"	INTEGER NOT NULL,
	PRIMARY KEY("id" AUTOINCREMENT),
	FOREIGN KEY("tsn") REFERENCES "taxonomic_units"("tsn"),
	FOREIGN KEY("germid") REFERENCES "germinationcodes"("id"),
	UNIQUE("germid","tsn")
);
CREATE TABLE IF NOT EXISTS "users" (
	"id"	INTEGER NOT NULL UNIQUE,
	"username"	TEXT NOT NULL UNIQUE,
	"pwhash"	TEXT NOT NULL,
	PRIMARY KEY("id" AUTOINCREMENT)
);
CREATE UNIQUE INDEX IF NOT EXISTS "collectionsamples_collection_sample" ON "seedcollectionsamples" (
	"collectionid",
	"sampleid"
);
