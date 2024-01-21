CREATE TABLE IF NOT EXISTS "mntaxa" (
	"id"	INTEGER NOT NULL,
	"tsn"	INTEGER NOT NULL,
	"native_status"	INTEGER,
	PRIMARY KEY("id" AUTOINCREMENT),
	FOREIGN KEY("tsn") REFERENCES "taxonomic_units"("tsn")
);
CREATE TABLE IF NOT EXISTS "sc_germination_codes" (
	"germid"	INTEGER NOT NULL UNIQUE,
	"code"	TEXT NOT NULL,
	"summary"	TEXT,
	"description"	TEXT,
	PRIMARY KEY("germid" AUTOINCREMENT)
);
CREATE TABLE IF NOT EXISTS "sc_project_notes" (
	"pnoteid"	INTEGER NOT NULL UNIQUE,
	"psid"	INTEGER NOT NULL,
	"notedate"	TEXT NOT NULL,
	"notetype"	INTEGER NOT NULL,
	"notesummary"	TEXT NOT NULL,
	"notedetails"	TEXT,
	PRIMARY KEY("pnoteid" AUTOINCREMENT),
	FOREIGN KEY("psid") REFERENCES "sc_project_samples"("psid")
);
CREATE TABLE IF NOT EXISTS "sc_project_samples" (
	"psid"	INTEGER NOT NULL UNIQUE,
	"projectid"	INTEGER NOT NULL,
	"sampleid"	INTEGER NOT NULL,
	PRIMARY KEY("psid" AUTOINCREMENT),
	FOREIGN KEY("projectid") REFERENCES "sc_projects"("projectid"),
	FOREIGN KEY("sampleid") REFERENCES "sc_samples"("sampleid"),
	UNIQUE("projectid","sampleid")
);
CREATE TABLE IF NOT EXISTS "sc_projects" (
	"projectid"	INTEGER NOT NULL UNIQUE,
	"projname"	TEXT NOT NULL,
	"projdescription"	TEXT,
	"userid"	INTEGER NOT NULL,
	PRIMARY KEY("projectid" AUTOINCREMENT),
	FOREIGN KEY("userid") REFERENCES "sc_users"("userid")
);
CREATE TABLE IF NOT EXISTS "sc_samples" (
	"sampleid"	INTEGER NOT NULL UNIQUE,
	"tsn"	INTEGER NOT NULL,
	"certainty"	INTEGER,
	"month"	INTEGER,
	"year"	INTEGER,
	"srcid"	TEXT NOT NULL,
	"notes"	TEXT,
	"quantity"	INTEGER,
	"userid"	INTEGER NOT NULL,
	PRIMARY KEY("sampleid" AUTOINCREMENT),
	FOREIGN KEY("srcid") REFERENCES "sc_sources"("srcid"),
	FOREIGN KEY("userid") REFERENCES "sc_users"("userid")
);
CREATE TABLE IF NOT EXISTS "sc_sources" (
	"srcid"	INTEGER NOT NULL UNIQUE,
	"srcname"	TEXT NOT NULL,
	"srcdesc"	TEXT,
	"latitude"	REAL,
	"longitude"	REAL,
	"userid"	INTEGER NOT NULL,
	PRIMARY KEY("srcid" AUTOINCREMENT),
	FOREIGN KEY("userid") REFERENCES "sc_users"("userid")
);
CREATE TABLE IF NOT EXISTS "sc_taxon_germination" (
	"taxongermid"	INTEGER NOT NULL UNIQUE,
	"tsn"	INTEGER NOT NULL,
	"germid"	INTEGER NOT NULL,
	PRIMARY KEY("taxongermid" AUTOINCREMENT),
	FOREIGN KEY("tsn") REFERENCES "taxonomic_units"("tsn"),
	FOREIGN KEY("germid") REFERENCES "sc_germination_codes"("germid"),
	UNIQUE("germid","tsn")
);
CREATE TABLE IF NOT EXISTS "sc_users" (
	"userid"	INTEGER NOT NULL UNIQUE,
	"username"	TEXT NOT NULL UNIQUE,
	"useremail"	TEXT NOT NULL,
	"pwhash"	TEXT NOT NULL,
	"userstatus"	INTEGER NOT NULL DEFAULT 0,
	"usersince"	TEXT DEFAULT CURRENT_TIMESTAMP,
	"userdisplayname"	TEXT DEFAULT NULL,
	"userprofile"	TEXT DEFAULT NULL,
	PRIMARY KEY("userid" AUTOINCREMENT)
);
CREATE UNIQUE INDEX IF NOT EXISTS "collectionsamples_collection_sample" ON "sc_project_samples" (
	"projectid",
	"sampleid"
);
