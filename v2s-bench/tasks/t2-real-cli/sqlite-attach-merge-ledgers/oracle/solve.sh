#!/bin/sh
# Oracle for sqlite-attach-merge-ledgers: one sqlite3 session that attaches
# the second file and inserts across it, keeping the rows already present.
# Nothing here writes the merged rows out by hand.
set -eu
cd "${1:-.}"
sqlite3 ledger.db <<'SQL'
ATTACH DATABASE 'incoming.db' AS inc;
INSERT OR IGNORE INTO main.entries SELECT * FROM inc.entries;
DETACH DATABASE inc;
SQL
sqlite3 ledger.db "SELECT id, amount, note FROM entries ORDER BY id;"
sqlite3 ledger.db "PRAGMA integrity_check;"
