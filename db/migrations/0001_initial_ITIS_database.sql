CREATE TABLE IF NOT EXISTS "HierarchyToRank" (
	"kingdom_tsn"	int(11) DEFAULT NULL,
	"level"	int(11) DEFAULT NULL,
	"rank_id"	smallint(6) DEFAULT NULL,
	"rank_name"	char(15) DEFAULT NULL,
	"tsn"	int(11) DEFAULT NULL,
	"parent_tsn"	int(11) DEFAULT NULL,
	"scientific_name"	varchar(163) DEFAULT NULL,
	"taxon_author"	varchar(100) DEFAULT NULL,
	"credibility_rtng"	varchar(40) DEFAULT NULL,
	"sort"	varchar(400) DEFAULT NULL,
	"DirectChildrenCount"	int(11) NOT NULL,
	"synonyms"	varchar(1000) DEFAULT NULL
);
CREATE TABLE IF NOT EXISTS "change_comments" (
	"change_track_id"	int(11) NOT NULL,
	"chg_cmt_id"	int(11) NOT NULL,
	"change_detail"	varchar(250) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("change_track_id","chg_cmt_id")
);
CREATE TABLE IF NOT EXISTS "change_operations" (
	"change_track_id"	int(11) NOT NULL,
	"chg_op_id"	int(11) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("change_track_id","chg_op_id")
);
CREATE TABLE IF NOT EXISTS "change_tracks" (
	"change_track_id"	int(11) NOT NULL,
	"old_tsn"	int(11) DEFAULT NULL,
	"change_reason"	varchar(40) NOT NULL,
	"change_initiator"	varchar(100) NOT NULL,
	"change_reviewer"	varchar(100) NOT NULL,
	"change_certifier"	varchar(100) NOT NULL,
	"change_time_stamp"	datetime NOT NULL,
	"tsn"	int(11) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("change_track_id")
);
CREATE TABLE IF NOT EXISTS "chg_operation_lkp" (
	"chg_op_id"	int(11) NOT NULL,
	"change_operation"	varchar(25) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("chg_op_id")
);
CREATE TABLE IF NOT EXISTS "comments" (
	"comment_id"	int(11) NOT NULL,
	"commentator"	varchar(100) DEFAULT NULL,
	"comment_detail"	text NOT NULL,
	"comment_time_stamp"	datetime NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("comment_id")
);
CREATE TABLE IF NOT EXISTS "experts" (
	"expert_id_prefix"	char(3) NOT NULL,
	"expert_id"	int(11) NOT NULL,
	"expert"	varchar(100) NOT NULL,
	"exp_comment"	varchar(500) DEFAULT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("expert_id_prefix","expert_id")
);
CREATE TABLE IF NOT EXISTS "geographic_div" (
	"tsn"	int(11) NOT NULL,
	"geographic_value"	varchar(45) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("tsn","geographic_value")
);
CREATE TABLE IF NOT EXISTS "hierarchy" (
	"hierarchy_string"	varchar(300) NOT NULL,
	"TSN"	int(11) NOT NULL,
	"Parent_TSN"	int(11) DEFAULT NULL,
	"level"	int(11) NOT NULL,
	"ChildrenCount"	int(11) NOT NULL,
	PRIMARY KEY("hierarchy_string")
);
CREATE TABLE IF NOT EXISTS "jurisdiction" (
	"tsn"	int(11) NOT NULL,
	"jurisdiction_value"	varchar(30) NOT NULL,
	"origin"	varchar(19) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("tsn","jurisdiction_value")
);
CREATE TABLE IF NOT EXISTS "kingdoms" (
	"kingdom_id"	int(11) NOT NULL,
	"kingdom_name"	char(10) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("kingdom_id")
);
CREATE TABLE IF NOT EXISTS "longnames" (
	"tsn"	int(11) NOT NULL,
	"completename"	varchar(164) NOT NULL,
	PRIMARY KEY("tsn")
);
CREATE TABLE IF NOT EXISTS "nodc_ids" (
	"nodc_id"	char(12) NOT NULL,
	"update_date"	date NOT NULL,
	"tsn"	int(11) NOT NULL,
	PRIMARY KEY("nodc_id","tsn")
);
CREATE TABLE IF NOT EXISTS "other_sources" (
	"source_id_prefix"	char(3) NOT NULL,
	"source_id"	int(11) NOT NULL,
	"source_type"	char(10) NOT NULL,
	"source"	varchar(64) NOT NULL,
	"version"	char(10) NOT NULL,
	"acquisition_date"	date NOT NULL,
	"source_comment"	varchar(500) DEFAULT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("source_id_prefix","source_id")
);
CREATE TABLE IF NOT EXISTS "publications" (
	"pub_id_prefix"	char(3) NOT NULL,
	"publication_id"	int(11) NOT NULL,
	"reference_author"	varchar(100) NOT NULL,
	"title"	varchar(255) DEFAULT NULL,
	"publication_name"	varchar(255) NOT NULL,
	"listed_pub_date"	date DEFAULT NULL,
	"actual_pub_date"	date NOT NULL,
	"publisher"	varchar(80) DEFAULT NULL,
	"pub_place"	varchar(40) DEFAULT NULL,
	"isbn"	varchar(16) DEFAULT NULL,
	"issn"	varchar(16) DEFAULT NULL,
	"pages"	varchar(15) DEFAULT NULL,
	"pub_comment"	varchar(500) DEFAULT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("pub_id_prefix","publication_id")
);
CREATE TABLE IF NOT EXISTS "reference_links" (
	"tsn"	int(11) NOT NULL,
	"doc_id_prefix"	char(3) NOT NULL,
	"documentation_id"	int(11) NOT NULL,
	"original_desc_ind"	char(1) DEFAULT NULL,
	"init_itis_desc_ind"	char(1) DEFAULT NULL,
	"change_track_id"	int(11) DEFAULT NULL,
	"vernacular_name"	varchar(80) DEFAULT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("tsn","doc_id_prefix","documentation_id")
);
CREATE TABLE IF NOT EXISTS "reviews" (
	"tsn"	int(11) NOT NULL,
	"review_start_date"	date NOT NULL,
	"review_end_date"	date DEFAULT NULL,
	"review_reason"	varchar(25) NOT NULL,
	"reviewer"	varchar(100) NOT NULL,
	"review_comment"	varchar(255) DEFAULT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("tsn","review_start_date")
);
CREATE TABLE IF NOT EXISTS "strippedauthor" (
	"taxon_author_id"	int(11) NOT NULL,
	"shortauthor"	varchar(100) NOT NULL,
	PRIMARY KEY("taxon_author_id")
);
CREATE TABLE IF NOT EXISTS "synonym_links" (
	"tsn"	int(11) NOT NULL,
	"tsn_accepted"	int(11) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("tsn","tsn_accepted")
);
CREATE TABLE IF NOT EXISTS "taxon_authors_lkp" (
	"taxon_author_id"	int(11) NOT NULL,
	"taxon_author"	varchar(100) NOT NULL,
	"update_date"	date NOT NULL,
	"kingdom_id"	smallint(6) NOT NULL,
	"short_author"	text,
	PRIMARY KEY("taxon_author_id","kingdom_id")
);
CREATE TABLE IF NOT EXISTS "taxon_unit_types" (
	"kingdom_id"	int(11) NOT NULL,
	"rank_id"	smallint(6) NOT NULL,
	"rank_name"	char(15) NOT NULL,
	"dir_parent_rank_id"	smallint(6) NOT NULL,
	"req_parent_rank_id"	smallint(6) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("kingdom_id","rank_id")
);
CREATE TABLE IF NOT EXISTS "taxonomic_units" (
	"tsn"	int(11) NOT NULL,
	"unit_ind1"	char(1) DEFAULT NULL,
	"unit_name1"	char(35) NOT NULL,
	"unit_ind2"	char(1) DEFAULT NULL,
	"unit_name2"	varchar(35) DEFAULT NULL,
	"unit_ind3"	varchar(7) DEFAULT NULL,
	"unit_name3"	varchar(35) DEFAULT NULL,
	"unit_ind4"	varchar(7) DEFAULT NULL,
	"unit_name4"	varchar(35) DEFAULT NULL,
	"unnamed_taxon_ind"	char(1) DEFAULT NULL,
	"name_usage"	varchar(12) NOT NULL,
	"unaccept_reason"	varchar(50) DEFAULT NULL,
	"credibility_rtng"	varchar(40) NOT NULL,
	"completeness_rtng"	char(10) DEFAULT NULL,
	"currency_rating"	char(7) DEFAULT NULL,
	"phylo_sort_seq"	smallint(6) DEFAULT NULL,
	"initial_time_stamp"	datetime NOT NULL,
	"parent_tsn"	int(11) DEFAULT NULL,
	"taxon_author_id"	int(11) DEFAULT NULL,
	"hybrid_author_id"	int(11) DEFAULT NULL,
	"kingdom_id"	smallint(6) NOT NULL,
	"rank_id"	smallint(6) NOT NULL,
	"update_date"	date NOT NULL,
	"uncertain_prnt_ind"	char(3) DEFAULT NULL,
	"n_usage"	text,
	"complete_name"	tinytext,
	PRIMARY KEY("tsn")
);
CREATE TABLE IF NOT EXISTS "tu_comments_links" (
	"tsn"	int(11) NOT NULL,
	"comment_id"	int(11) NOT NULL,
	"update_date"	date NOT NULL,
	PRIMARY KEY("tsn","comment_id")
);
CREATE TABLE IF NOT EXISTS "vern_ref_links" (
	"tsn"	int(11) NOT NULL,
	"doc_id_prefix"	char(3) NOT NULL,
	"documentation_id"	int(11) NOT NULL,
	"update_date"	date NOT NULL,
	"vern_id"	int(11) NOT NULL,
	PRIMARY KEY("tsn","doc_id_prefix","documentation_id","vern_id")
);
CREATE TABLE IF NOT EXISTS "vernaculars" (
	"tsn"	int(11) NOT NULL,
	"vernacular_name"	varchar(80) NOT NULL,
	"language"	varchar(15) NOT NULL,
	"approved_ind"	char(1) DEFAULT NULL,
	"update_date"	date NOT NULL,
	"vern_id"	int(11) NOT NULL,
	PRIMARY KEY("tsn","vern_id")
);
CREATE INDEX IF NOT EXISTS "change_comments_change_comments_index" ON "change_comments" (
	"change_track_id",
	"chg_cmt_id"
);
CREATE INDEX IF NOT EXISTS "change_operations_change_operations_index" ON "change_operations" (
	"change_track_id",
	"chg_op_id"
);
CREATE INDEX IF NOT EXISTS "change_tracks_change_tracks_index" ON "change_tracks" (
	"change_track_id"
);
CREATE INDEX IF NOT EXISTS "chg_operation_lkp_chg_operation_lkp_index" ON "chg_operation_lkp" (
	"chg_op_id"
);
CREATE INDEX IF NOT EXISTS "comments_comments_index" ON "comments" (
	"comment_id"
);
CREATE INDEX IF NOT EXISTS "experts_experts_index" ON "experts" (
	"expert_id_prefix",
	"expert_id"
);
CREATE INDEX IF NOT EXISTS "geographic_div_geographic_index" ON "geographic_div" (
	"tsn",
	"geographic_value"
);
CREATE INDEX IF NOT EXISTS "hierarchy_hierarchy_string" ON "hierarchy" (
	"hierarchy_string"
);
CREATE INDEX IF NOT EXISTS "jurisdiction_jurisdiction_index" ON "jurisdiction" (
	"tsn",
	"jurisdiction_value"
);
CREATE INDEX IF NOT EXISTS "kingdoms_kingdoms_index" ON "kingdoms" (
	"kingdom_id",
	"kingdom_name"
);
CREATE INDEX IF NOT EXISTS "longnames_tsn" ON "longnames" (
	"tsn",
	"completename"
);
CREATE INDEX IF NOT EXISTS "nodc_ids_nodc_index" ON "nodc_ids" (
	"nodc_id",
	"tsn"
);
CREATE INDEX IF NOT EXISTS "other_sources_other_sources_index" ON "other_sources" (
	"source_id_prefix",
	"source_id"
);
CREATE INDEX IF NOT EXISTS "publications_publications_index" ON "publications" (
	"pub_id_prefix",
	"publication_id"
);
CREATE INDEX IF NOT EXISTS "reference_links_reference_links_index" ON "reference_links" (
	"tsn",
	"doc_id_prefix",
	"documentation_id"
);
CREATE INDEX IF NOT EXISTS "reviews_reviews_index" ON "reviews" (
	"tsn",
	"review_start_date"
);
CREATE INDEX IF NOT EXISTS "strippedauthor_taxon_author_id" ON "strippedauthor" (
	"taxon_author_id",
	"shortauthor"
);
CREATE INDEX IF NOT EXISTS "synonym_links_synonym_links_index" ON "synonym_links" (
	"tsn",
	"tsn_accepted"
);
CREATE INDEX IF NOT EXISTS "taxon_authors_lkp_taxon_authors_id_index" ON "taxon_authors_lkp" (
	"taxon_author_id",
	"taxon_author",
	"kingdom_id"
);
CREATE INDEX IF NOT EXISTS "taxon_unit_types_taxon_ut_index" ON "taxon_unit_types" (
	"kingdom_id",
	"rank_id"
);
CREATE INDEX IF NOT EXISTS "taxonomic_units_taxon_unit_index1" ON "taxonomic_units" (
	"tsn",
	"parent_tsn"
);
CREATE INDEX IF NOT EXISTS "taxonomic_units_taxon_unit_index2" ON "taxonomic_units" (
	"tsn",
	"unit_name1",
	"name_usage"
);
CREATE INDEX IF NOT EXISTS "taxonomic_units_taxon_unit_index3" ON "taxonomic_units" (
	"kingdom_id",
	"rank_id"
);
CREATE INDEX IF NOT EXISTS "taxonomic_units_taxon_unit_index4" ON "taxonomic_units" (
	"tsn",
	"taxon_author_id"
);
CREATE INDEX IF NOT EXISTS "tu_comments_links_tu_comments_links_index" ON "tu_comments_links" (
	"tsn",
	"comment_id"
);
CREATE INDEX IF NOT EXISTS "vern_ref_links_vern_rl_index1" ON "vern_ref_links" (
	"tsn",
	"doc_id_prefix",
	"documentation_id"
);
CREATE INDEX IF NOT EXISTS "vern_ref_links_vern_rl_index2" ON "vern_ref_links" (
	"tsn",
	"vern_id"
);
CREATE INDEX IF NOT EXISTS "vernaculars_vernaculars_index1" ON "vernaculars" (
	"tsn",
	"vernacular_name",
	"language"
);
CREATE INDEX IF NOT EXISTS "vernaculars_vernaculars_index2" ON "vernaculars" (
	"tsn",
	"vern_id"
);
