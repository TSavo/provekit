from types import SimpleNamespace

from recognize_demo_python import run_demo


class FakeResponse:
    status_code = 200

    def json(self):
        return {"event_type": "signup", "user": "alice", "payload": {"age": 30}}


def test_run_demo_round_trips_http_json_through_sqlite(monkeypatch, tmp_path):
    requests = SimpleNamespace(
        request=lambda method, url, **kwargs: FakeResponse(),
        get=lambda url: SimpleNamespace(status_code=200),
    )
    monkeypatch.setitem(__import__("sys").modules, "requests", requests)

    summary = run_demo(
        "https://events.example.test/signup",
        str(tmp_path / "events.sqlite3"),
    )

    assert summary == {
        "rowid": 1,
        "user": "alice",
        "type": "signup",
        "payload": {"age": 30},
    }
