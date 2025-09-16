BEGIN TRANSACTION;
-- canada wild rye
INSERT INTO "taxonomic_units" VALUES (40683,NULL,'Elymus',NULL,'canadensis',NULL,NULL,NULL,NULL,NULL,'accepted',NULL,'TWG standards met',NULL,NULL,0,'1996-06-13 14:51:08',40677,41302,0,3,220,'2010-11-30','No','accepted','Elymus canadensis');
INSERT INTO "taxonomic_units" VALUES (40677,NULL,'Elymus',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2016',0,'1996-06-13 14:51:08',40351,41302,0,3,180,'2016-05-25','No','accepted','Elymus');
INSERT INTO "taxonomic_units" VALUES (40351,NULL,'Poaceae',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'1996-06-13 14:51:08',846620,0,0,3,140,'2014-12-22','No','accepted','Poaceae');
INSERT INTO "taxonomic_units" VALUES (846620,NULL,'Poales',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2012-03-29 09:35:44',846542,0,0,3,100,'2014-12-22','No','accepted','Poales');
INSERT INTO "taxonomic_units" VALUES (846542,NULL,'Lilianae',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2012-03-29 09:35:39',18063,0,0,3,90,'2022-02-25','No','accepted','Lilianae');
INSERT INTO "taxonomic_units" VALUES (18063,NULL,'Magnoliopsida',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'1996-06-13 14:51:08',846504,0,0,3,60,'2022-02-25','No','accepted','Magnoliopsida');
INSERT INTO "taxonomic_units" VALUES (846504,NULL,'Spermatophytina',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2012-03-29 09:35:31',846496,0,0,3,40,'2022-02-25','No','accepted','Spermatophytina');
INSERT INTO "taxonomic_units" VALUES (846496,NULL,'Tracheophyta',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2012-03-29 09:35:27',954900,0,0,3,30,'2022-02-25','No','accepted','Tracheophyta');
INSERT INTO "taxonomic_units" VALUES (954900,NULL,'Embryophyta',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2014-12-22 14:36:37',846494,0,0,3,27,'2022-02-25','No','accepted','Embryophyta');
INSERT INTO "taxonomic_units" VALUES (846494,NULL,'Streptophyta',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2012-03-29 09:35:27',954898,0,0,3,25,'2022-02-25','No','accepted','Streptophyta');
INSERT INTO "taxonomic_units" VALUES (954898,NULL,'Viridiplantae',NULL,NULL,NULL,NULL,NULL,NULL,'N','accepted',NULL,'TWG standards met','partial','2014',0,'2014-12-22 14:36:34',202422,0,0,3,20,'2022-02-25','No','accepted','Viridiplantae');
INSERT INTO "taxonomic_units" VALUES (202422,NULL,'Plantae',NULL,NULL,NULL,NULL,NULL,NULL,NULL,'accepted',NULL,'TWG standards met','partial','2004',0,'1996-06-13 14:51:08',0,0,0,3,10,'2004-02-23',NULL,'accepted','Plantae');

-- hierarchy
INSERT INTO "hierarchy" VALUES ("202422",202422,0,0,167883);
INSERT INTO "hierarchy" VALUES ("202422-954898",954898,202422,1,165209);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494",846494,954898,2,162780);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900",954900,846494,3,160624);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496",846496,954900,4,143095);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496-846504",846504,846496,5,137721);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496-846504-18063",18063,846504,6,136300);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496-846504-18063-846542",846542,18063,7,39943);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496-846504-18063-846542-846620",846620,846542,8,31991);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496-846504-18063-846542-846620-40351",40351,846620,9,28393);
INSERT INTO "hierarchy" VALUES ("202422-954898-846494-954900-846496-846504-18063-846542-846620-40351-40677",40677,40351,10,800);
COMMIT;
