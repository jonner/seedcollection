DROP VIEW vsamples;
CREATE VIEW IF NOT EXISTS vsamples (sampleid, tsn, rank, parentid, srcid, srcname, srcdesc, complete_name, unit_name1, unit_name2, unit_name3, seq, quantity, month, year, notes, certainty, cnames, userid) AS
SELECT S.sampleid,
       T.tsn,
       T.rank_id,
       T.parent_tsn,
       L.srcid,
       L.srcname,
       L.srcdesc,
       T.complete_name,
       T.unit_name1,
       T.unit_name2,
       T.unit_name3,
       T.phylo_sort_seq,
       quantity,
       MONTH,
       YEAR,
       notes,
       certainty,
       GROUP_CONCAT(V.vernacular_name, "@"),
       U.userid
FROM sc_samples S
INNER JOIN taxonomic_units T ON T.tsn=S.tsn
INNER JOIN sc_sources L ON L.srcid=S.srcid
INNER JOIN sc_users U ON U.userid=S.userid
LEFT JOIN
  (SELECT *
   FROM vernaculars
   WHERE (LANGUAGE="English"
          OR LANGUAGE="unspecified") ) V ON V.tsn=T.tsn
GROUP BY S.sampleid,
         T.tsn
