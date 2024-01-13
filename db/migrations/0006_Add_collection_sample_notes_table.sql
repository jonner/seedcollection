CREATE TABLE "sc_collection_sample_notes" (
	"id"	INTEGER NOT NULL UNIQUE,
	"csid"	INTEGER NOT NULL,
	"date"	TEXT NOT NULL,
	"kind"	INTEGER NOT NULL,
	"summary"	TEXT NOT NULL,
	"details"	TEXT,
	PRIMARY KEY("id" AUTOINCREMENT),
	FOREIGN KEY("csid") REFERENCES "sc_collection_samples"("id")
);
