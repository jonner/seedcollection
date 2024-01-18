BEGIN TRANSACTION;
INSERT INTO "sc_projects" VALUES(1, "First Collection", "This is a description of the first collection", 1);
INSERT INTO "sc_projects" VALUES(2, "Second Collection", NULL, 1);
INSERT INTO "sc_samples" VALUES(1, 40683, 1, 9, 2023, 1, "These are some notes", NULL, 1);
INSERT INTO "sc_samples" VALUES(2, 40683, 1, NULL, 2022, 2, NULL, 240, 1);
INSERT INTO "sc_samples" VALUES(3, 43254, 1, NULL, 2022, 2, NULL, 240, 1);
INSERT INTO "sc_project_samples" VALUES(1, 1, 1);
INSERT INTO "sc_project_samples" VALUES(2, 1, 2);
INSERT INTO "sc_project_samples" VALUES(3, 2, 3);
INSERT INTO "sc_project_samples" VALUES(4, 2, 1);
INSERT INTO "sc_project_notes" VALUES(1, 1, "2023-12-25", 1, "Note summary 1", "note details 1");
INSERT INTO "sc_project_notes" VALUES(2, 1, "2023-12-27", 1, "Note summary 2", "note details 2");
COMMIT;

