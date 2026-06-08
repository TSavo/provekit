package persist

import (
	"database/sql"

	_ "github.com/sugar/recognize-demo-go/internal/memsqlite"
)

func OpenStore(driver string, dsn string) *sql.DB {
	return mustDB(sql.Open(driver, dsn))
}

func mustDB(db *sql.DB, err error) *sql.DB {
	if err != nil {
		panic(err)
	}
	return db
}
