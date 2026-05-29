from types import SimpleNamespace

from recognize_demo_python.ingest import fetch_event_response, fetch_event_status


class FakeResponse:
    def __init__(self, payload=None, status_code=200):
        self._payload = payload or {}
        self.status_code = status_code

    def json(self):
        return self._payload


def test_fetch_event_response_uses_requests_request(monkeypatch):
    calls = []

    def request(method, url, **kwargs):
        calls.append((method, url, kwargs))
        return FakeResponse({"event_type": "signup", "user": "alice", "payload": {"age": 30}})

    monkeypatch.setitem(__import__("sys").modules, "requests", SimpleNamespace(request=request))

    response = fetch_event_response(
        "GET",
        "https://events.example.test/signup",
        {"Accept": "application/json"},
        None,
    )

    assert response.json()["user"] == "alice"
    assert calls == [
        (
            "GET",
            "https://events.example.test/signup",
            {"headers": {"Accept": "application/json"}, "data": None},
        )
    ]


def test_fetch_event_status_uses_requests_get(monkeypatch):
    calls = []

    def get(url):
        calls.append(url)
        return FakeResponse(status_code=204)

    monkeypatch.setitem(__import__("sys").modules, "requests", SimpleNamespace(get=get))

    assert fetch_event_status("https://events.example.test/health") == 204
    assert calls == ["https://events.example.test/health"]
