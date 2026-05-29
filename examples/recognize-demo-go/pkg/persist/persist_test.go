package persist

import "testing"

func TestOpenStoreReturnsUsableInMemorySQLite(t *testing.T) {
	db := OpenStore("sqlite", ":memory:")
	defer db.Close()

	if _, err := db.Exec(`create table events (body text not null)`); err != nil {
		t.Fatalf("create table: %v", err)
	}
	if _, err := db.Exec(`insert into events (body) values (?)`, `{"ok":true}`); err != nil {
		t.Fatalf("insert event: %v", err)
	}

	var body string
	if err := db.QueryRow(`select body from events`).Scan(&body); err != nil {
		t.Fatalf("select body: %v", err)
	}
	if body != `{"ok":true}` {
		t.Fatalf("body = %q", body)
	}
}
