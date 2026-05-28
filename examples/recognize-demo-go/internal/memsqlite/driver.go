package memsqlite

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"fmt"
	"io"
	"strings"
	"sync"
)

func init() {
	sql.Register("sqlite", driverImpl{})
}

type driverImpl struct{}

func (driverImpl) Open(name string) (driver.Conn, error) {
	return &conn{store: storeFor(name)}, nil
}

var stores = struct {
	sync.Mutex
	byName map[string]*store
}{
	byName: map[string]*store{},
}

func storeFor(name string) *store {
	stores.Lock()
	defer stores.Unlock()
	if existing := stores.byName[name]; existing != nil {
		return existing
	}
	next := &store{}
	stores.byName[name] = next
	return next
}

type store struct {
	sync.Mutex
	rows []string
}

type conn struct {
	store *store
}

func (c *conn) Prepare(query string) (driver.Stmt, error) {
	return nil, fmt.Errorf("memsqlite does not implement prepared statements: %s", query)
}

func (c *conn) Close() error {
	return nil
}

func (c *conn) Begin() (driver.Tx, error) {
	return tx{}, nil
}

func (c *conn) ExecContext(_ context.Context, query string, args []driver.NamedValue) (driver.Result, error) {
	normalized := normalize(query)
	switch {
	case strings.HasPrefix(normalized, "create table events"):
		c.store.Lock()
		c.store.rows = nil
		c.store.Unlock()
		return driver.RowsAffected(0), nil
	case strings.HasPrefix(normalized, "insert into events"):
		if len(args) != 1 {
			return nil, fmt.Errorf("insert expects one argument, got %d", len(args))
		}
		body, ok := args[0].Value.(string)
		if !ok {
			return nil, fmt.Errorf("insert argument has type %T, want string", args[0].Value)
		}
		c.store.Lock()
		c.store.rows = append(c.store.rows, body)
		c.store.Unlock()
		return driver.RowsAffected(1), nil
	default:
		return nil, fmt.Errorf("unsupported exec query: %s", query)
	}
}

func (c *conn) QueryContext(_ context.Context, query string, _ []driver.NamedValue) (driver.Rows, error) {
	if !strings.HasPrefix(normalize(query), "select body from events") {
		return nil, fmt.Errorf("unsupported query: %s", query)
	}
	c.store.Lock()
	rows := append([]string(nil), c.store.rows...)
	c.store.Unlock()
	return &bodyRows{rows: rows}, nil
}

func normalize(query string) string {
	return strings.Join(strings.Fields(strings.ToLower(query)), " ")
}

type tx struct{}

func (tx) Commit() error {
	return nil
}

func (tx) Rollback() error {
	return nil
}

type bodyRows struct {
	rows []string
	idx  int
}

func (r *bodyRows) Columns() []string {
	return []string{"body"}
}

func (r *bodyRows) Close() error {
	return nil
}

func (r *bodyRows) Next(dest []driver.Value) error {
	if r.idx >= len(r.rows) {
		return io.EOF
	}
	dest[0] = r.rows[r.idx]
	r.idx++
	return nil
}
