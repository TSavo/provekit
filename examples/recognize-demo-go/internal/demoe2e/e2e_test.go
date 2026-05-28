package demoe2e

import (
	"io"
	"net/http"
	"strings"
	"testing"

	"github.com/provekit/recognize-demo-go/pkg/ingest"
	"github.com/provekit/recognize-demo-go/pkg/persist"
)

func TestRecognizeDemoRoundTrip(t *testing.T) {
	oldTransport := http.DefaultTransport
	t.Cleanup(func() {
		http.DefaultTransport = oldTransport
	})
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
		t.Fatalf("ReadAll: %v", err)
	}

	db := persist.OpenStore("sqlite", ":memory:")
	defer db.Close()

	if _, err := db.Exec(`create table events (body text not null)`); err != nil {
		t.Fatalf("create table: %v", err)
	}
	if _, err := db.Exec(`insert into events (body) values (?)`, string(body)); err != nil {
		t.Fatalf("insert event: %v", err)
	}

	var got string
	if err := db.QueryRow(`select body from events`).Scan(&got); err != nil {
		t.Fatalf("select body: %v", err)
	}
	if got != `{"user":"alice","type":"signup"}` {
		t.Fatalf("stored body = %q", got)
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) {
	return f(r)
}
