import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { handleLogin } from "./login.ts";
import { sessionEmail } from "./auth/session.ts";

const PORT = 3000;
const page = readFileSync(new URL("../public/index.html", import.meta.url), "utf8");

function readBody(req: import("node:http").IncomingMessage): Promise<string> {
  return new Promise((resolve) => {
    let data = "";
    req.on("data", (chunk) => (data += chunk));
    req.on("end", () => resolve(data));
  });
}

// The web server: serves the login page and handles POST /login.
const server = createServer(async (req, res) => {
  if (req.method === "POST" && req.url === "/login") {
    const outcome = handleLogin(JSON.parse((await readBody(req)) || "{}"));
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (outcome.sessionId) headers["set-cookie"] = `session=${outcome.sessionId}; HttpOnly; Path=/`;
    res.writeHead(outcome.status, headers);
    res.end(JSON.stringify({ message: outcome.message }));
    return;
  }

  if (req.url === "/me") {
    const cookie = /session=([a-f0-9]+)/.exec(req.headers.cookie ?? "")?.[1];
    const email = sessionEmail(cookie);
    res.writeHead(email ? 200 : 401, { "content-type": "application/json" });
    res.end(JSON.stringify({ email: email ?? null }));
    return;
  }

  res.writeHead(200, { "content-type": "text/html" });
  res.end(page);
});

server.listen(PORT, () => console.log(`Demo app on http://localhost:${PORT}`));
