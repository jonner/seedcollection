BEGIN TRANSACTION;
INSERT INTO "sc_users" VALUES (1,'testuser','test@domain.com', 'fake-password-hash', 0, "2024-01-01 11:22:33", NULL, NULL);
INSERT INTO "sc_users" VALUES (2,'test.user2','test2@domain.org', 'faux-password-hash', 1, "2023-10-20 11:00:55", "Cool Display Name", NULL);
COMMIT;
