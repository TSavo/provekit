from __future__ import annotations

import aiosqlite
from typing import Any, Coroutine, TypedDict


class User(TypedDict):
    id: int
    name: str
    email: str


class RequestLike(TypedDict):
    path: str


class Response(TypedDict):
    status: int
    body: str


DB_PATH = "users.sqlite"


def _row_to_user(row: aiosqlite.Row | None) -> User | None:
    if row is None:
        return None
    return {
        "id": int(row["id"]),
        "name": str(row["name"]),
        "email": str(row["email"]),
    }


async def get_user_by_id(id: int) -> Coroutine[Any, Any, User]:
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        async with db.execute(
            "SELECT id, name, email FROM users WHERE id = ?",
            (id,),
        ) as cursor:
            row = await cursor.fetchone()
    user = _row_to_user(row)
    if user is None:
        raise RuntimeError(f"missing user {id}")
    return user


async def get_all_users() -> Coroutine[Any, Any, list[User]]:
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        async with db.execute("SELECT id, name, email FROM users ORDER BY id") as cursor:
            rows = await cursor.fetchall()
    return [
        {
            "id": int(row["id"]),
            "name": str(row["name"]),
            "email": str(row["email"]),
        }
        for row in rows
    ]


async def count_users() -> Coroutine[Any, Any, int]:
    async with aiosqlite.connect(DB_PATH) as db:
        db.row_factory = aiosqlite.Row
        async with db.execute("SELECT count(*) AS count FROM users") as cursor:
            row = await cursor.fetchone()
    return int(row["count"] if row is not None else 0)


async def render_users_page() -> Coroutine[Any, Any, str]:
    users = await get_all_users()
    items = "".join(f"<li>{exported_formatter(user)}</li>" for user in users)
    return f"<ul>{items}</ul>"


async def render_dashboard() -> Coroutine[Any, Any, str]:
    page = await render_users_page()
    count = await count_users()
    return f'<section data-count="{count}">{page}</section>'


async def handle_request(req: RequestLike) -> Coroutine[Any, Any, Response]:
    if req["path"] != "/users":
        return {"status": 404, "body": "not found"}
    return {"status": 200, "body": await render_dashboard()}


def exported_formatter(u: User) -> str:
    return f"{u['name']} <{u['email']}> ({count_users()} users)"


async def record_event(user_id: int, kind: str) -> Coroutine[Any, Any, int]:
    async with aiosqlite.connect(DB_PATH) as db:
        async with db.execute(
            "INSERT INTO events (user_id, kind) VALUES (?, ?)",
            (user_id, kind),
        ) as cursor:
            await db.commit()
            return int(cursor.lastrowid or 0)
