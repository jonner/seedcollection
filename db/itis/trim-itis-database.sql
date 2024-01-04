-- remove unnecessary tables
DROP TABLE HierarchyToRank;
DROP TABLE change_comments;
DROP TABLE change_operations;
DROP TABLE change_tracks;
DROP TABLE chg_operation_lkp;
DROP TABLE comments;
DROP TABLE experts;
DROP TABLE geographic_div;
DROP TABLE jurisdiction;
DROP TABLE nodc_ids;
DROP TABLE other_sources;
DROP TABLE publications;
DROP TABLE reference_links;
DROP TABLE reviews;
DROP TABLE tu_comments_links;
DROP TABLE vern_ref_links;
-- remove all non-plant data
DELETE FROM hierarchy WHERE ROWID IN (SELECT H.ROWID FROM hierarchy H INNER JOIN taxonomic_units T ON H.tsn=T.tsn WHERE T.kingdom_id<>3);
DELETE FROM longnames WHERE ROWID IN (SELECT H.ROWID FROM longnames H INNER JOIN taxonomic_units T ON H.tsn=T.tsn WHERE T.kingdom_id<>3);
DELETE FROM strippedauthor WHERE ROWID IN (SELECT H.ROWID FROM strippedauthor H INNER JOIN taxonomic_units T ON H.taxon_author_id=T.taxon_author_id WHERE T.kingdom_id<>3);
DELETE FROM synonym_links WHERE ROWID IN (SELECT H.ROWID FROM synonym_links H INNER JOIN taxonomic_units T ON H.tsn=T.tsn WHERE T.kingdom_id<>3);
DELETE FROM taxon_authors_lkp WHERE ROWID IN (SELECT H.ROWID FROM taxon_authors_lkp H INNER JOIN taxonomic_units T ON H.taxon_author_id=T.taxon_author_id WHERE T.kingdom_id<>3);
DELETE FROM vernaculars WHERE ROWID IN (SELECT H.ROWID FROM vernaculars H INNER JOIN taxonomic_units T ON H.tsn=T.tsn WHERE T.kingdom_id<>3);
DELETE FROM taxonomic_units WHERE kingdom_id<>3;
UPDATE taxonomic_units SET phylo_sort_seq = H.rowid FROM (SELECT ROW_NUMBER() OVER (ORDER BY hierarchy_string) AS rowid, tsn FROM hierarchy) as H WHERE H.tsn=taxonomic_units.tsn
VACUUM;
