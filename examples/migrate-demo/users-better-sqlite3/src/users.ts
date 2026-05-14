import Database from "better-sqlite3";

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

const db = new Database("users.sqlite");

export function getUserById(id: number): User {
  const row = db
    .prepare("SELECT id, name, email FROM users WHERE id = ?")
    .get(id) as User | undefined;
  if (!row) {
    throw new Error(`missing user ${id}`);
  }
  return row;
}

export function getAllUsers(): User[] {
  return db
    .prepare("SELECT id, name, email FROM users ORDER BY id")
    .all() as User[];
}

export function countUsers(): number {
  const row = db.prepare("SELECT count(*) AS count FROM users").get() as { count: number };
  return row.count;
}

export function renderUsersPage(): string {
  const users = getAllUsers();
  return `<ul>${users.map((user) => `<li>${exportedFormatter(user)}</li>`).join("")}</ul>`;
}

export function renderDashboard(): string {
  const page = renderUsersPage();
  const count = countUsers();
  return `<section data-count="${count}">${page}</section>`;
}

export async function handleRequest(req: RequestLike): Promise<Response> {
  if (req.path !== "/users") {
    return { status: 404, body: "not found" };
  }
  return { status: 200, body: renderDashboard() };
}

// provekit:migrate-public-api sync-return
export function exportedFormatter(u: User): string {
  return `${u.name} <${u.email}> (${countUsers()} users)`;
}

export function recordEvent(userId: number, kind: string): number {
  const result = db
    .prepare("INSERT INTO events (user_id, kind) VALUES (?, ?)")
    .run(userId, kind);
  return Number(result.lastInsertRowid);
}
