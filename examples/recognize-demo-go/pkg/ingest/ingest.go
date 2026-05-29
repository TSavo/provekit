package ingest

import "net/http"

func FetchURL(target string) *http.Response {
	return mustResponse(http.Get(target))
}

func mustResponse(resp *http.Response, err error) *http.Response {
	if err != nil {
		panic(err)
	}
	return resp
}
