// Run with the bundled Deno runtime. Edge handles the site's browser-only
// login, search server actions and signed single-track download links.
const origin = "https://livedjservice.com";
const [query, outputDir] = Deno.args;

function edgePath(): string {
  const env = Deno.env.get("DJDROP_EDGE_PATH");
  if (env) return env;
  for (const root of [Deno.env.get("PROGRAMFILES(X86)"), Deno.env.get("PROGRAMFILES"), Deno.env.get("LOCALAPPDATA")]) {
    if (!root) continue;
    const path = `${root}\\Microsoft\\Edge\\Application\\msedge.exe`;
    try { Deno.statSync(path); return path; } catch { /* try next location */ }
  }
  throw new Error("Microsoft Edge is required for LiveDJService downloads");
}

class DevTools {
  private nextId = 0;
  private pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  constructor(private socket: WebSocket) {
    socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const waiting = this.pending.get(message.id);
      if (!waiting) return;
      this.pending.delete(message.id);
      if (message.error) waiting.reject(new Error(message.error.message));
      else waiting.resolve(message.result);
    };
    socket.onclose = () => {
      for (const waiting of this.pending.values()) waiting.reject(new Error("Browser disconnected"));
      this.pending.clear();
    };
  }
  static async connect(url: string): Promise<DevTools> {
    const socket = new WebSocket(url);
    const { promise, resolve, reject } = Promise.withResolvers<void>();
    socket.onopen = () => resolve();
    socket.onerror = () => reject(new Error("Could not connect to Edge"));
    await promise;
    return new DevTools(socket);
  }
  async send(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
    const id = ++this.nextId;
    const { promise: reply, resolve, reject } = Promise.withResolvers<unknown>();
    this.pending.set(id, { resolve, reject });
    this.socket.send(JSON.stringify({ id, method, params }));
    return await reply;
  }
  async evaluate<T>(expression: string): Promise<T> {
    const result = await this.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true }) as {
      result: { value: T }; exceptionDetails?: { text: string };
    };
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result.value;
  }
  close(): void { this.socket.close(); }
}

async function until<T>(check: () => Promise<T | undefined | false>, description: string, timeout = 45000): Promise<T> {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value;
    const { promise, resolve } = Promise.withResolvers<void>();
    setTimeout(resolve, 250);
    await promise;
  }
  throw new Error(`Timed out waiting for ${description}`);
}

function normalized(text: string): string {
  return text.toLowerCase().normalize("NFKC").replace(/[^\p{L}\p{N}]+/gu, " ").trim();
}

export function exactMatches<T extends { title: string }>(tracks: T[], query: string): T[] {
  const wanted = normalized(query);
  const full = tracks.filter((track) => normalized(track.title) === wanted);
  if (full.length) return full;
  return tracks.filter((track) => {
    const separator = track.title.indexOf("—");
    return separator >= 0 && normalized(track.title.slice(separator + 1)) === wanted;
  });
}

async function run(): Promise<void> {
  const username = Deno.env.get("LIVEDJ_USERNAME") ?? "";
  const password = Deno.env.get("LIVEDJ_PASSWORD") ?? "";
  if (!username || !password) throw new Error("Set both LiveDJService credential references in Settings");
  if (!query || !outputDir) throw new Error("Missing LiveDJService search query or output directory");
  let search = query;
  if (/^https?:\/\//i.test(query)) {
    const url = new URL(query);
    if (!["livedjservice.com", "www.livedjservice.com"].includes(url.hostname) ||
        url.pathname !== "/search" || !url.searchParams.get("q")) {
      throw new Error("LiveDJService section URLs contain multiple tracks; enter an artist and exact track title instead");
    }
    search = url.searchParams.get("q")!;
  }

  const profile = await Deno.makeTempDir({ prefix: "djdrop-lds-profile-" });
  const downloads = await Deno.makeTempDir({ prefix: "djdrop-lds-download-" });
  let browser: Deno.ChildProcess | undefined;
  let devtools: DevTools | undefined;
  try {
    const flags = ["--headless=new", "--disable-blink-features=AutomationControlled",
      "--no-first-run", "--no-default-browser-check", "--disable-extensions",
      "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"];
    browser = new Deno.Command(edgePath(), {
      args: flags,
      stdin: "null", stdout: "null", stderr: "null",
    }).spawn();
    const portFile = `${profile}\\DevToolsActivePort`;
    const port = await until(async () => {
      try { return Number((await Deno.readTextFile(portFile)).split("\n")[0]) || undefined; }
      catch { return undefined; }
    }, "Edge startup", 20000);
    const targets = await until(async () => {
      const response = await fetch(`http://127.0.0.1:${port}/json/list`);
      return (await response.json() as { type: string; webSocketDebuggerUrl: string }[]).find((t) => t.type === "page");
    }, "Edge page");
    devtools = await DevTools.connect(targets.webSocketDebuggerUrl);
    await devtools.send("Page.enable");
    await devtools.send("Runtime.enable");
    // Match the installed browser's normal user agent rather than advertising
    // \"HeadlessChrome\" to the site. Keep the browser version in sync automatically.
    const version = await devtools.send("Browser.getVersion") as { userAgent: string };
    await devtools.send("Network.setUserAgentOverride", {
      userAgent: version.userAgent.replace("HeadlessChrome/", "Chrome/"),
      platform: "Windows",
    });
    await devtools.send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: downloads });
    const navigate = async (url: string) => {
      await devtools!.send("Page.navigate", { url });
      await until(() => devtools!.evaluate<boolean>(`location.href === ${JSON.stringify(url)} && document.readyState === 'complete'`), "page load");
    };

    console.log(JSON.stringify({ stage: "LiveDJService login" }));
    await navigate(`${origin}/login`);
    try {
      await until(() => devtools!.evaluate<boolean>("!!document.querySelector('input[name=email]')"), "login form", 15000);
    } catch {
      const page = await devtools.evaluate<{ title: string; text: string }>(
        "({ title: document.title, text: document.body?.innerText?.slice(0, 200) ?? '' })");
      throw new Error(`LiveDJService login page unavailable: ${page.title}: ${page.text}`);
    }
    await devtools.evaluate(`(() => {
      const email = document.querySelector('input[name=email]');
      const pass = document.querySelector('input[name=password]');
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      set.call(email, ${JSON.stringify(username)}); email.dispatchEvent(new Event('input', { bubbles: true }));
      set.call(pass, ${JSON.stringify(password)}); pass.dispatchEvent(new Event('input', { bubbles: true }));
      email.form.requestSubmit();
    })()`);
    await until(async () => {
      const state = await devtools!.evaluate<{ path: string; error: string }>(`({
        path: location.pathname,
        error: document.querySelector('[role=alert]')?.textContent ?? ''
      })`);
      if (state.error) throw new Error(`LiveDJService login: ${state.error}`);
      return state.path !== "/login";
    }, "LiveDJService sign-in");

    console.log(JSON.stringify({ stage: "LiveDJService search" }));
    await navigate(`${origin}/search?q=${encodeURIComponent(search)}`);
    await until(async () => {
      const state = await devtools!.evaluate<{ loading: boolean; count: number; text: string }>(`({
        loading: document.body.innerText.includes('Loading search results'),
        count: document.querySelectorAll('button[aria-label^="Select "]').length,
        text: document.body.innerText
      })`);
      return !state.loading && (state.count > 0 || /no results/i.test(state.text));
    }, "LiveDJService search results");
    const tracks = await devtools.evaluate<{ title: string; label: string }[]>(`[...document.querySelectorAll('button[aria-label^="Select "]')].map(button => ({
      title: button.parentElement.querySelector('.grow > div > div:nth-child(2)')?.textContent?.trim() ?? '',
      label: button.getAttribute('aria-label')
    }))`);
    const matches = exactMatches(tracks, search);
    if (matches.length !== 1) {
      const suggestions = tracks.slice(0, 5).map((track) => track.title).join("; ");
      throw new Error(matches.length > 1
        ? `Several LiveDJService versions match '${search}'; include the exact edit/remix name`
        : `No exact LiveDJService match for '${search}'${suggestions ? `. Results: ${suggestions}` : ""}`);
    }

    await devtools.evaluate(`document.querySelector('[role=switch]')?.click()`);
    const label = matches[0].label.replace(/^Select /, "Download ");
    await until(() => devtools!.evaluate<boolean>(`!!document.querySelector('button[aria-label=${JSON.stringify(label)}]')`), "single-track controls");
    console.log(JSON.stringify({ stage: "LiveDJService download" }));
    await devtools.evaluate(`document.querySelector('button[aria-label=${JSON.stringify(label)}]').click()`);

    const file = await until(async () => {
      let result: string | undefined;
      for await (const entry of Deno.readDir(downloads)) {
        if (entry.isFile && /\.(mp3|wav|flac|aiff?|m4a)$/i.test(entry.name)) result = entry.name;
        if (entry.name.endsWith(".crdownload")) return undefined;
      }
      return result;
    }, "LiveDJService audio transfer", 120000);
    const from = `${downloads}\\${file}`;
    if ((await Deno.stat(from)).size === 0) throw new Error("LiveDJService returned an empty audio file");
    let destination = `${outputDir}\\${file}`;
    try {
      await Deno.stat(destination);
      const dot = file.lastIndexOf(".");
      destination = `${outputDir}\\${file.slice(0, dot)}-${crypto.randomUUID().slice(0, 8)}${file.slice(dot)}`;
    } catch { /* name is free */ }
    await Deno.rename(from, destination);
    console.log(JSON.stringify({ file_path: destination }));
  } finally {
    devtools?.close();
    if (browser) {
      try { browser.kill(); } catch { /* already exited */ }
      await browser.status;
    }
    await Deno.remove(profile, { recursive: true });
    await Deno.remove(downloads, { recursive: true });
  }
}

if (import.meta.main) {
  try { await run(); }
  catch (error) { console.error(error instanceof Error ? error.message : String(error)); Deno.exitCode = 1; }
}
