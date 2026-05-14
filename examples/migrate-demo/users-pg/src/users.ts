import { Pool } from "pg";

export interface User {
  id: number;
  name: string;
  email: string;
}

export interface RequestLike {
  path: string;
}

export interface Response {
  status: number;
  body: string;
}

const pool = new Pool({});

export async function getUserById(id: number): Promise<User> {
  const result = await pool.query<User>(
    "SELECT id, name, email FROM users WHERE id = $1",
    [id],
  );
  const row = result.rows[0];
  if (!row) {
    throw new Error(`missing user ${id}`);
  }
  return row;
}

export async function getAllUsers(): Promise<User[]> {
  const result = await pool.query<User>(
    "SELECT id, name, email FROM users ORDER BY id",
    [],
  );
  return result.rows;
}

export async function countUsers(): Promise<number> {
  const result = await pool.query<{ count: number | string }>(
    "SELECT count(*) AS count FROM users",
    [],
  );
  return Number(result.rows[0]?.count ?? 0);
}

export async function renderUsersPage(): Promise<string> {
  const users = await getAllUsers();
  return `<ul>${users.map((user) => `<li>${exportedFormatter(user)}</li>`).join("")}</ul>`;
}

export async function renderDashboard(): Promise<string> {
  const page = await renderUsersPage();
  const count = await countUsers();
  return `<section data-count="${count}">${page}</section>`;
}

export async function handleRequest(req: RequestLike): Promise<Response> {
  if (req.path !== "/users") {
    return { status: 404, body: "not found" };
  }
  return { status: 200, body: await renderDashboard() };
}

export function exportedFormatter(u: User): string {
  return `${u.name} <${u.email}> (${countUsers()} users)`;
}

export async function recordEvent(userId: number, kind: string): Promise<number> {
  const result = await pool.query<{ id: number }>(
    "INSERT INTO events (user_id, kind) VALUES ($1, $2) RETURNING id",
    [userId, kind],
  );
  return Number(result.rows[0]?.id ?? 0);
}
