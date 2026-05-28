package stdlibshim

import (
	"database/sql"
	"net/http"
)

//provekit:sugar(concept="concept:http-get", library="recognize-demo-go-stdlib-http", version="1", family="concept:family:http")
func HTTPGet(url string) *http.Response {
	return mustResponse(http.Get(url))
}

//provekit:sugar(concept="concept:sql-open", library="recognize-demo-go-stdlib-http", version="1", family="concept:family:sql")
func SQLOpen(driverName string, dataSourceName string) *sql.DB {
	return mustDB(sql.Open(driverName, dataSourceName))
}

func mustResponse(resp *http.Response, err error) *http.Response {
	if err != nil {
		panic(err)
	}
	return resp
}

func mustDB(db *sql.DB, err error) *sql.DB {
	if err != nil {
		panic(err)
	}
	return db
}
