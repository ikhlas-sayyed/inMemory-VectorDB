import { createConnection, type Socket } from "node:net";

export type Request =
  | { type: "insert"; vector: number[]; data: number[] }
  | { type: "search"; vector: number[]; k: number }
  | { type: "remove"; id: number }
  | { type: "stats" };

export interface SearchResult {
  id: number;
  data: number[];
  similarity: number;
}

export interface Response {
  ok: boolean;
  id?: number;
  results?: SearchResult[];
  count?: number;
  dim?: number;
  error?: string;
}

type Pending = {
  resolve: (response: Response) => void;
  reject: (error: Error) => void;
};

export class Client {
  private readonly socket: Socket;
  private buffer = "";
  private pending: Pending[] = [];
  private closedError: Error | undefined;

  private constructor(socket: Socket) {
    this.socket = socket;
    socket.setEncoding("utf8");
    socket.on("data", (chunk: string) => this.read(chunk));
    socket.on("error", (error) => this.fail(error));
    socket.on("close", () => {
      this.fail(new Error("server closed the connection"));
    });
  }

  static connect(address: string): Promise<Client> {
    const { host, port } = parseAddress(address);
    return new Promise((resolve, reject) => {
      const socket = createConnection({ host, port });
      const onError = (error: Error) => {
        socket.destroy();
        reject(error);
      };
      socket.once("error", onError);
      socket.once("connect", () => {
        socket.off("error", onError);
        resolve(new Client(socket));
      });
    });
  }

  request(request: Request): Promise<Response> {
    if (this.closedError) {
      return Promise.reject(this.closedError);
    }

    return new Promise((resolve, reject) => {
      this.pending.push({ resolve, reject });
      this.socket.write(`${JSON.stringify(request)}\n`, (error) => {
        if (error) {
          this.fail(error);
        }
      });
    });
  }

  async insert(vector: number[], data: Uint8Array = new Uint8Array()): Promise<number> {
    const response = await this.request({ type: "insert", vector, data: Array.from(data) });
    if (response.id === undefined) {
      throw new Error("insert response did not include an id");
    }
    return response.id;
  }

  async search(vector: number[], k: number): Promise<SearchResult[]> {
    const response = await this.request({ type: "search", vector, k });
    return response.results ?? [];
  }

  async remove(id: number): Promise<void> {
    await this.request({ type: "remove", id });
  }

  async stats(): Promise<{ count: number; dim?: number }> {
    const response = await this.request({ type: "stats" });
    return { count: response.count ?? 0, dim: response.dim };
  }

  close(): void {
    this.socket.end();
  }

  private read(chunk: string): void {
    this.buffer += chunk;
    let newline = this.buffer.indexOf("\n");
    while (newline !== -1) {
      const line = this.buffer.slice(0, newline);
      this.buffer = this.buffer.slice(newline + 1);
      this.resolveLine(line);
      newline = this.buffer.indexOf("\n");
    }
  }

  private resolveLine(line: string): void {
    const pending = this.pending.shift();
    if (!pending) {
      return;
    }
    try {
      const response = JSON.parse(line) as Response;
      if (!response.ok) {
        pending.reject(new Error(response.error ?? "unknown server error"));
      } else {
        pending.resolve(response);
      }
    } catch (error) {
      pending.reject(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private fail(error: Error): void {
    if (this.closedError) {
      return;
    }
    this.closedError = error;
    for (const pending of this.pending.splice(0)) {
      pending.reject(error);
    }
  }
}

function parseAddress(address: string): { host: string; port: number } {
  const ipv6 = address.match(/^\[([^\]]+)\]:(\d+)$/);
  const separator = ipv6 ? undefined : address.lastIndexOf(":");
  const host = ipv6?.[1] ?? (separator === undefined ? undefined : address.slice(0, separator));
  const portText = ipv6?.[2] ?? (separator === undefined ? undefined : address.slice(separator + 1));
  const port = portText === undefined ? Number.NaN : Number(portText);
  if (!host || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`invalid database address: ${address}`);
  }
  return { host, port };
}
