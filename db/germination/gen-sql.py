#!/usr/bin/env python
import sqlite3
import logging
import os
import csv
import argparse

debuglevel = os.environ.get("DEBUG")
if debuglevel == "1":
    debuglevel = "DEBUG"
logging.basicConfig(level=debuglevel)

KINGDOM_PLANTAE = 3
RANK_GENUS = 180
RANK_SPECIES = 220
RANK_SUBSPECIES = 230
RANK_VARIETY = 240

CSV_FIELDS = ["X", "genus", "X", "species", "subttype", "subtaxa", "germcode"]


def debug_row(row):
    logging.debug("Row : {}".format(row))
    if row:
        rowvals = []
        for k in row.keys():
            rowvals.append("{}={}".format(k, row[k]))
        logging.debug(", ".join(rowvals))


def find_genus_synonym(cursor, genus):
    logging.info("Looking for a synonym for {}".format(genus))
    res = cursor.execute(
        """SELECT S.tsn_accepted as tsn from taxonomic_units T
            INNER JOIN synonym_links S ON T.tsn=S.tsn
            WHERE name_usage="not accepted" AND unit_name1=?
            AND kingdom_id=? and rank_id=?""",
        (genus, KINGDOM_PLANTAE, RANK_GENUS),
    )
    row = res.fetchone()
    debug_row(row)
    if row:
        logging.info("Found synonym {}, looking up info about it".format(row["tsn"]))
        res = cursor.execute(
            """SELECT T.tsn, T.unit_name1 as genus, T.rank_id
                FROM taxonomic_units T
                WHERE T.tsn=? AND name_usage="accepted" AND kingdom_id=?""",
            (row["tsn"], KINGDOM_PLANTAE),
        )
        row = res.fetchone()
        if row:
            debug_row(row)
            return row["genus"]
    return None


def displayname(name1, name2, name3):
    return " ".join(item for item in [name1, name2, name3] if item)


def find_synonym(cursor, name1, name2, name3, rank):
    dname = displayname(name1, name2, name3)
    logging.info("Looking for a synonym for {}".format(dname))
    res = None
    if rank == RANK_SPECIES:
        res = cursor.execute(
            """SELECT S.tsn_accepted as tsn from taxonomic_units T
                INNER JOIN synonym_links S ON T.tsn=S.tsn
                WHERE name_usage="not accepted" AND unit_name1=? and unit_name2=?
                AND kingdom_id=? and rank_id=?""",
            (name1, name2, KINGDOM_PLANTAE, rank),
        )
    else:
        res = cursor.execute(
            """SELECT S.tsn_accepted as tsn from taxonomic_units T
                INNER JOIN synonym_links S ON T.tsn=S.tsn
                WHERE name_usage="not accepted" AND unit_name1=?
                AND unit_name2=? AND unit_name3=? AND kingdom_id=? and rank_id=?""",
            (name1, name2, name3, KINGDOM_PLANTAE, rank),
        )
    row = res.fetchone()
    debug_row(row)
    if row:
        logging.info("Found synonym {}, looking up info about it".format(row["tsn"]))
        res = cursor.execute(
            """SELECT T.tsn, T.complete_name,
                GROUP_CONCAT(V.vernacular_name) AS common_names, T.rank_id
                FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
                WHERE T.tsn=? AND name_usage="accepted" AND kingdom_id=?
                GROUP BY T.tsn""",
            (row["tsn"], KINGDOM_PLANTAE),
        )
        return res.fetchone()
    return None


def find_possibilities(cursor, name1, name2, name3, rank):
    logging.info("Looking for other possibilities for {} {}".format(name1, name2))
    if rank == RANK_SPECIES:
        res = cursor.execute(
            """SELECT * from taxonomic_units T
                WHERE unit_name1 Like ? OR unit_name2 LIKE ? AND kingdom_id=?""",
            (name1, name2, KINGDOM_PLANTAE),
        )
    else:
        res = cursor.execute(
            """SELECT * from taxonomic_units T
                WHERE unit_name1 Like ? OR unit_name2 LIKE ?
                OR unit_name3 LIKE ? AND kingdom_id=?""",
            (name1, name2, name3, KINGDOM_PLANTAE),
        )
    if res is not None:
        rows = res.fetchall()
        if not rows:
            return False
        logging.warning(
            "Unable to find an exact match for {}. Set DEBUG=1 to view {} possible matches".format(
                (name1, name2, name3, rank), len(rows)
            )
        )
        logging.debug(" Possibilities:")
        for row in rows:
            debug_row(row)
            logging.debug("   - {}: {}".format(row["tsn"], row["complete_name"]))
        return True
    else:
        return False


def get_germ_code_id(cursor, germcode):
    res = cursor.execute(
        """SELECT germid FROM sc_germination_codes WHERE code=?""", (germcode,)
    )
    row = res.fetchone()

    if row is not None:
        return row["germid"]
    return None


def get_taxon(cursor, name1, name2, name3, rank):
    synonym = False
    dname = displayname(name1, name2, name3)
    logging.info("Looking up information for {}".format((name1, name2, name3, rank)))
    res = None
    if rank == RANK_SPECIES:
        res = cursor.execute(
            """SELECT T.tsn, rank_id, complete_name,
                GROUP_CONCAT(V.vernacular_name) as common_names
                FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
                WHERE unit_name1=? AND unit_name2=? AND name_usage="accepted"
                AND kingdom_id=? AND rank_id=? GROUP BY T.tsn""",
            (name1, name2, KINGDOM_PLANTAE, rank),
        )
    else:
        res = cursor.execute(
            """SELECT T.tsn, rank_id, complete_name,
                GROUP_CONCAT(V.vernacular_name) as common_names
                FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
                WHERE unit_name1=? AND unit_name2=? AND unit_name3=?
                AND name_usage="accepted" AND kingdom_id=? AND rank_id=?
                GROUP BY T.tsn""",
            (name1, name2, name3, KINGDOM_PLANTAE, rank),
        )
    row = res.fetchone()
    if row is None:
        synonym = True
        row = find_synonym(cursor, name1, name2, name3, rank)

    if row is not None:
        cname = row["common_names"] or "no common name known"
        prefix = "*" if synonym else ""
        logging.info(
            "{}{} is <{}> {} ({})".format(
                prefix, dname, row["tsn"], row["complete_name"], cname
            )
        )
        return row["tsn"]
    return None


def handle_taxa_list(cursor, reader):
    taxa = []
    for row in reader:
        ind1 = row[CSV_FIELDS[0]].strip()
        name1 = row[CSV_FIELDS[1]].strip()
        ind2 = row[CSV_FIELDS[2]].strip()
        name2 = row[CSV_FIELDS[3]].strip()
        ind3 = row[CSV_FIELDS[4]].strip()
        name3 = row[CSV_FIELDS[5]].strip()
        germcode = str(row[CSV_FIELDS[6]].strip())

        # skip hybrids for now
        if ind1 == "X" or ind2 == "X":
            logging.info("skipping hybrid for now")
            continue

        # lookup germ code id
        code = get_germ_code_id(cursor, germcode)

        rank = RANK_SPECIES
        if ind3 == "var.":
            rank = RANK_VARIETY
        elif ind3 == "subsp.":
            rank == RANK_SUBSPECIES
        tsn = get_taxon(cursor, name1, name2, name3, rank)
        if tsn is not None:
            taxa.append((tsn, code))
            continue

        new_genus = find_genus_synonym(cursor, name1)
        if new_genus:
            logging.info(
                "genus {} is a synonym for {}, using new name {} {}".format(
                    name1, new_genus, new_genus, name2
                )
            )
            tsn = get_taxon(cursor, new_genus, name2, name3, rank)
            if tsn is not None:
                taxa.append((tsn, code))
                continue

        if not find_possibilities(cursor, name1, name2, name3, rank):
            logging.warning("unable to find species {} {}".format(name1, name2))
    return taxa


def check_fieldnames(fieldnames):
    if (len(reader.fieldnames)) != len(CSV_FIELDS):
        raise RuntimeError("Expected 9 fields, found {}".format(len(reader.fieldnames)))
    for i in range(len(fieldnames)):
        if CSV_FIELDS[i] != fieldnames[i]:
            raise RuntimeError(
                "Field name mismatch. expected field named '{}' in column {}, found '{}'".format(
                    CSV_FIELDS[i], i, fieldnames[i]
                )
            )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("specieslist")
    parser.add_argument("-d", "--db", default="ITIS.sqlite")
    args = parser.parse_args()

    dburl = "file:{}?mode=ro".format(args.db)
    dbconn = sqlite3.connect(dburl, uri=True)
    dbconn.row_factory = sqlite3.Row
    csvfile = open(args.specieslist)
    reader = csv.DictReader(csvfile)
    try:
        check_fieldnames(reader.fieldnames)
    except RuntimeError as e:
        print("Failed to parse input file: {}".format(e))
        exit(1)

    cursor = dbconn.cursor()
    taxa = handle_taxa_list(cursor, reader)
    cursor = None
    dbconn = None
    if taxa:
        outdb = sqlite3.connect(":memory:")
        logging.info("Adding {} items to the database".format(len(taxa)))
        outcursor = outdb.cursor()
        outcursor.execute("""
            CREATE TABLE IF NOT EXISTS "sc_taxon_germination" (
                "taxongermid"	INTEGER NOT NULL UNIQUE,
                "tsn"	INTEGER NOT NULL,
                "germid"	INTEGER NOT NULL,
                PRIMARY KEY("taxongermid" AUTOINCREMENT),
                FOREIGN KEY("tsn") REFERENCES "taxonomic_units"("tsn"),
                FOREIGN KEY("germid") REFERENCES "germinationcodes"("germid"),
                UNIQUE("germid","tsn") ON CONFLICT IGNORE
            )""")
        outcursor.executemany(
            "INSERT INTO sc_taxon_germination ('tsn', 'germid') VALUES (?, ?)",
            taxa,
        )
        outdb.commit()
        for line in outdb.iterdump():
            print(line)
