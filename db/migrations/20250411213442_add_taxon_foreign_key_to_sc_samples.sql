-- hack to allow disabling foreign keys temporarily
COMMIT TRANSACTION;

PRAGMA foreign_keys=OFF;

BEGIN TRANSACTION;

CREATE TABLE "sc_samples_new" (
	"sampleid"	INTEGER NOT NULL UNIQUE,
	"tsn"	INTEGER NOT NULL,
	"certainty"	INTEGER,
	"month"	INTEGER,
	"year"	INTEGER,
	"srcid"	INTEGER NOT NULL,
	"notes"	TEXT,
	"quantity"	REAL,
	"userid"	INTEGER NOT NULL,
	PRIMARY KEY("sampleid" AUTOINCREMENT),
	FOREIGN KEY("srcid") REFERENCES "sc_sources"("srcid"),
	FOREIGN KEY("tsn") REFERENCES "taxonomic_units"("tsn"),
	FOREIGN KEY("userid") REFERENCES "sc_users"("userid")
);

INSERT INTO sc_samples_new SELECT * FROM sc_samples;
DROP VIEW vsamples;
DROP TABLE sc_samples;
ALTER TABLE sc_samples_new RENAME TO sc_samples;

CREATE VIEW vsamples (sampleid, tsn, rank, parentid, srcid, srcname, srcdesc, complete_name, unit_name1, unit_name2, unit_name3, seq, quantity, month, year, notes, certainty, cnames, userid) AS
SELECT S.sampleid,
       T.tsn,
       T.rank_id,
       T.parent_tsn,
       L.srcid,
       L.srcname,
       L.srcdesc,
       T.complete_name,
       T.unit_name1,
       T.unit_name2,
       T.unit_name3,
       T.phylo_sort_seq,
       quantity,
       MONTH,
       YEAR,
       notes,
       certainty,
       GROUP_CONCAT(V.vernacular_name, "@"),
       U.userid
FROM sc_samples S
INNER JOIN taxonomic_units T ON T.tsn=S.tsn
INNER JOIN sc_sources L ON L.srcid=S.srcid
INNER JOIN sc_users U ON U.userid=S.userid
LEFT JOIN
  (SELECT *
   FROM vernaculars
   WHERE (LANGUAGE="English"
          OR LANGUAGE="unspecified") ) V ON V.tsn=T.tsn
GROUP BY S.sampleid,
         T.tsn;

PRAGMA foreign_key_check;
COMMIT TRANSACTION;

PRAGMA foreign_keys=ON;

-- hack to let the migrator finish the transaction
BEGIN TRANSACTION;
