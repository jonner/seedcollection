ALTER TABLE sc_samples ADD COLUMN "userid" INTEGER REFERENCES "sc_users"("id");
ALTER TABLE sc_locations ADD COLUMN "userid" INTEGER REFERENCES "sc_users"("id");
ALTER TABLE sc_collections ADD COLUMN "userid" INTEGER REFERENCES "sc_users"("id");
