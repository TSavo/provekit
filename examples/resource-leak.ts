import * as fs from "fs";

export function processFile(path: string): string[] {
  const fd = fs.openSync(path, "r");
  const buffer = Buffer.alloc(1024);
  const results: string[] = [];

  console.log(`Processing file: ${path}, fd=${fd}`);

  let bytesRead = fs.readSync(fd, buffer);
  while (bytesRead > 0) {
    const line = buffer.toString("utf-8", 0, bytesRead).trim();
    if (line.includes("ERROR")) {
      console.log(`Found error in ${path}: ${line}`);
      return results;  // BUG: returns without closing fd
    }
    results.push(line);
    bytesRead = fs.readSync(fd, buffer);
  }

  fs.closeSync(fd);
  console.log(`Finished processing ${path}, ${results.length} lines`);
  return results;
}

export function connectAndQuery(host: string, query: string): any {
  const connection = createConnection(host);
  console.log(`Connected to ${host}, running query`);

  const result = connection.execute(query);

  if (result.error) {
    console.log(`Query failed: ${result.error}`);
    return null;  // BUG: connection never closed on error path
  }

  connection.close();
  console.log(`Query complete, ${result.rows.length} rows`);
  return result.rows;
}

function createConnection(host: string): any {
  return {
    execute: (q: string) => ({ rows: [], error: null }),
    close: () => {},
  };
}
