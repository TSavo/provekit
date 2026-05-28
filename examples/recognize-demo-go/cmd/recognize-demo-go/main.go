package main

import (
	"fmt"
	"io"
	"log"
	"net/http"
	"strings"

	"github.com/provekit/recognize-demo-go/pkg/ingest"
	"github.com/provekit/recognize-demo-go/pkg/persist"
)

func main() {
	http.DefaultTransport = roundTripFunc(func(r *http.Request) (*http.Response, error) {
		return &http.Response{
			StatusCode: http.StatusOK,
			Header:     make(http.Header),
			Body:       io.NopCloser(strings.NewReader(`{"user":"alice","type":"signup"}`)),
			Request:    r,
		}, nil
	})

	resp := ingest.FetchURL("https://demo.local/event")
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		log.Fatalf("read body: %v", err)
	}

	db := persist.OpenStore("sqlite", ":memory:")
	defer db.Close()

	if _, err := db.Exec(`create table events (body text not null)`); err != nil {
		log.Fatalf("create table: %v", err)
	}
	if _, err := db.Exec(`insert into events (body) values (?)`, string(body)); err != nil {
		log.Fatalf("insert event: %v", err)
	}

	var stored string
	if err := db.QueryRow(`select body from events`).Scan(&stored); err != nil {
		log.Fatalf("select event: %v", err)
	}

	fmt.Printf("recognize-demo-go round-trip: status=%d body=%s\n", resp.StatusCode, stored)
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) {
	return f(r)
}
