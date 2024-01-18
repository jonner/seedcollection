BEGIN TRANSACTION;
INSERT INTO "sc_sources" VALUES (1,'Test source 1','description 1',40.123,-90.123,1);
INSERT INTO "sc_sources" VALUES (2,'Test source 2','description 2',34.123,-83.123,1);
COMMIT;
