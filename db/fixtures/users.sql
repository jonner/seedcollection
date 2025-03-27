BEGIN TRANSACTION;
-- the provided hash represents the password 'topsecret123'
INSERT INTO "sc_users" VALUES (1,'testuser','test@domain.com', '$argon2id$v=19$m=19456,t=2,p=1$VKVM6uVHKql3CJyxm9e6TA$68w0NBt9Q3C5FtK4yO7LCEK1uFPqB73B5MR1fSg4Z0I', 0, "2024-01-01 11:22:33", NULL, NULL);
INSERT INTO "sc_users" VALUES (2,'test.user2','test2@domain.org', 'faux-password-hash', 0, "2023-10-20 11:00:55", "Cool Display Name", NULL);
COMMIT;
