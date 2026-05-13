# HTTP Concept Shape Examples

These examples are illustrative bindings for Bridge C. They do not mint sugar dictionaries.

| Language surface | Request binding | Response binding |
| --- | --- | --- |
| C libcurl | `CURLOPT_CUSTOMREQUEST` or default method maps to `method`; `CURLOPT_URL` maps to `url`; `curl_slist` request headers map to `headers`; upload callbacks or POST fields map to `body`. | `CURLINFO_RESPONSE_CODE` maps to `status`; received header callbacks map to `headers`; write callbacks map to `body`. |
| Java `java.net.http` | `HttpRequest.method`, `HttpRequest.uri`, `HttpRequest.headers`, and `BodyPublisher` bind to the four `concept:http-request` slots. | `HttpResponse.statusCode`, `HttpResponse.headers`, and `BodyHandler` output bind to the three `concept:http-response` slots. |
| Python `urllib.request` | `urllib.request.Request.get_method()`, `full_url`, `header_items()`, and `data` bind to the four `concept:http-request` slots. | `HTTPResponse.status`, `headers`, and `read()` or streaming reads bind to the three `concept:http-response` slots. |

The same `concept:http-request` CID is the shared input for these request surfaces. The same `concept:http-response` CID is the shared input for these response surfaces.
