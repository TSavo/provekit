declare module "pg" {
  export interface QueryResult<T> {
    rowCount: number | null;
    rows: T[];
  }

  export class Pool {
    constructor(config?: unknown);
    query<T = unknown>(text: string, values?: unknown[]): Promise<QueryResult<T>>;
  }
}
