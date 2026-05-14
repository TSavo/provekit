PRAGMA page_size = 512;

CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT NOT NULL
);

CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id INTEGER NOT NULL,
  kind TEXT NOT NULL
);

INSERT INTO users (id, name, email) VALUES
  (1, 'Ada Lovelace', 'ada@example.test'),
  (2, 'Grace Hopper', 'grace@example.test'),
  (3, 'Katherine Johnson', 'katherine@example.test'),
  (4, 'Edsger Dijkstra', 'edsger@example.test'),
  (5, 'Barbara Liskov', 'barbara@example.test');

INSERT INTO events (user_id, kind) VALUES
  (1, 'login'),
  (2, 'view'),
  (1, 'logout');
