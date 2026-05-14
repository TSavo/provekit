from __future__ import annotations

import sqlite3
from typing import TypedDict


class User(TypedDict):
    id: int
    name: str
    email: str


class RequestLike(TypedDict):
    path: str


class Response(TypedDict):
    status: int
    body: str


db = sqlite3.connect("users.sqlite")
db.row_factory = sqlite3.Row


def _row_to_user(row: sqlite3.Row | None) -> User | None:
    if row is None:
        return None
    return {
        "id": int(row["id"]),
        "name": str(row["name"]),
        "email": str(row["email"]),
    }


def get_user_by_id(id: int) -> User:
    row = db.execute(
        "SELECT id, name, email FROM users WHERE id = ?",
        (id,),
    ).fetchone()
    user = _row_to_user(row)
    if user is None:
        raise RuntimeError(f"missing user {id}")
    return user


def get_all_users() -> list[User]:
    rows = db.execute("SELECT id, name, email FROM users ORDER BY id").fetchall()
    return [
        {
            "id": int(row["id"]),
            "name": str(row["name"]),
            "email": str(row["email"]),
        }
        for row in rows
    ]


def count_users() -> int:
    row = db.execute("SELECT count(*) AS count FROM users").fetchone()
    return int(row["count"] if row is not None else 0)


def render_users_page() -> str:
    users = get_all_users()
    items = "".join(f"<li>{exported_formatter(user)}</li>" for user in users)
    return f"<ul>{items}</ul>"


def render_dashboard() -> str:
    page = render_users_page()
    count = count_users()
    return f'<section data-count="{count}">{page}</section>'


def handle_request(req: RequestLike) -> Response:
    if req["path"] != "/users":
        return {"status": 404, "body": "not found"}
    return {"status": 200, "body": render_dashboard()}


def exported_formatter(u: User) -> str:
    return f"{u['name']} <{u['email']}> ({count_users()} users)"


def record_event(user_id: int, kind: str) -> int:
    cursor = db.execute(
        "INSERT INTO events (user_id, kind) VALUES (?, ?)",
        (user_id, kind),
    )
    db.commit()
    return int(cursor.lastrowid or 0)
