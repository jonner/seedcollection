BEGIN TRANSACTION;
INSERT INTO "sc_locations" VALUES (1,'Test location 1','description 1',40.123,-90.123,1);
INSERT INTO "sc_locations" VALUES (2,'Test location 2','description 2',34.123,-83.123,1);
COMMIT;
