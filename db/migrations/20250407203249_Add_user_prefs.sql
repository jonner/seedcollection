CREATE TABLE "sc_user_prefs" (
    "prefid" INTEGER NOT NULL UNIQUE,
    "userid" INTEGER NOT NULL,
    "pagesize" INTEGER NOT NULL,
    PRIMARY KEY("prefid" AUTOINCREMENT),
    FOREIGN KEY("userid") REFERENCES "sc_users"("userid")
);
