package ingest

import (
	"io"
	"net/http"
	"strings"
	"testing"
)

func TestFetchURLReturnsHTTPResponse(t *testing.T) {
	oldTransport := http.DefaultTransport
	t.Cleanup(func() {
		http.DefaultTransport = oldTransport
	})
	http.DefaultTransport = roundTripFunc(func(r *http.Request) (*http.Response, error) {
		if r.URL.String() != "https://demo.local/event" {
			t.Fatalf("url = %q, want https://demo.local/event", r.URL.String())
		}
		return &http.Response{
			StatusCode: http.StatusAccepted,
			Header:     make(http.Header),
			Body:       io.NopCloser(strings.NewReader(`{"user":"alice","type":"signup"}`)),
			Request:    r,
		}, nil
	})

	resp := FetchURL("https://demo.local/event")
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusAccepted {
		t.Fatalf("status = %d, want %d", resp.StatusCode, http.StatusAccepted)
	}
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("ReadAll: %v", err)
	}
	if string(body) != `{"user":"alice","type":"signup"}` {
		t.Fatalf("body = %q", body)
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) {
	return f(r)
}
