BEGIN TRANSACTION;
INSERT INTO "sc_collections" VALUES(1, "First Collection", "This is a description of the first collection", 1);
INSERT INTO "sc_collections" VALUES(2, "Second Collection", NULL, 1);
INSERT INTO "sc_samples" VALUES(1, 40683, 1, 9, 2023, 1, "These are some notes", NULL, 1);
INSERT INTO "sc_samples" VALUES(2, 40683, 1, NULL, 2022, 2, NULL, 240, 1);
INSERT INTO "sc_samples" VALUES(3, 43254, 1, NULL, 2022, 2, NULL, 240, 1);
INSERT INTO "sc_collection_samples" VALUES(1, 1, 1);
INSERT INTO "sc_collection_samples" VALUES(2, 1, 2);
INSERT INTO "sc_collection_samples" VALUES(3, 2, 3);
INSERT INTO "sc_collection_sample_notes" VALUES(1, 1, "2024-01-16", 3, "summary 1", "details 1");
INSERT INTO "sc_collection_sample_notes" VALUES(2, 1, "2024-01-12", 3, "summary 2", NULL);
INSERT INTO "sc_collection_sample_notes" VALUES(3, 2, "2024-01-16", 1, "summary 3", "details 3");
COMMIT;

