ALTER TABLE sc_collection_sample_notes RENAME COLUMN id TO csnoteid;
ALTER TABLE sc_collection_samples RENAME COLUMN id TO csid;
ALTER TABLE sc_collections RENAME COLUMN id TO collectionid;
ALTER TABLE sc_germination_codes RENAME COLUMN id TO germid;
ALTER TABLE sc_samples RENAME COLUMN id TO sampleid;
ALTER TABLE sc_taxon_germination RENAME COLUMN id TO taxongermid;
ALTER TABLE sc_users RENAME COLUMN id TO userid;
