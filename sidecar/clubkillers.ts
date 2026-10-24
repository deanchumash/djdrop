// Run with the bundled Deno runtime. Club Killers' web client talks to a plain
// JSON API; we sign in, find the exact song, and take its "Intro - Dirty" file.
const site = "https://djtools.clubkillers.com";
const fileHost = "https://stage.clubkillers.com/downloadSong.php";
const DOWNLOAD_LIMIT = 3;
const INTRO_DIRTY = 4;
const TYPE_DIRS: Record<number, string> = {
  1: "acapella", 2: "instrumental", 3: "intro_clean", 4: "intro_dirty", 5: "clean", 6: "dirty", 7: "other",
};

export interface SongVersion {
  version_label: string;
  version: number;
  file_name: string;
  uploaded_mp3_name: string;
  download: boolean[];
}
export interface Song { title: string; artist: string; version?: SongVersion[]; song_section_id?: number }

/** Lower-case words with accents and punctuation removed ("ADÉLA" -> "adela"). */
export function words(text: string): string[] {
  return text.normalize("NFKD").replace(/\p{M}/gu, "").toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
}

const sameWords = (a: string[], b: string[]) => a.length === b.length && [...a].sort().join(" ") === [...b].sort().join(" ");

/** Songs whose title, or artist plus title in any order, is exactly the query. */
export function exactSongs(songs: Song[], query: string): Song[] {
  const wanted = words(query);
  return songs.filter((song) =>
    sameWords(words(song.title), wanted) || sameWords(words(`${song.artist} ${song.title}`), wanted)
  );
}

export const usedDownloads = (version: SongVersion) => version.download.filter(Boolean).length;

/** The dirty intro to take: "CK Intro - Dirty" first, then any other dirty intro. Files at the limit are skipped. */
export function pickVersion(song: Song): SongVersion | undefined {
  const usable = (song.version ?? []).filter((v) =>
    (v.version === INTRO_DIRTY || /intro/i.test(v.version_label) && /dirty/i.test(v.version_label)) &&
    usedDownloads(v) < DOWNLOAD_LIMIT
  );
  return usable.find((v) => /\bck\b/i.test(v.version_label)) ?? usable[0];
}

const describe = (song: Song) =>
  (song.version ?? []).map((v) => `${v.version_label} ${usedDownloads(v)}/${DOWNLOAD_LIMIT}`).join(", ") || "no files";

class Api {
  constructor(private base: string, private appKey: string) {}
  async call(endpoint: string, body: Record<string, unknown>): Promise<{ status: number; body: unknown }> {
    const response = await fetch(`${this.base}${endpoint}?store_id=1&lang_code=en`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${this.appKey}` },
      body: JSON.stringify(body),
    });
    return { status: response.status, body: await response.json().catch(() => ({})) as unknown };
  }
}

/** The web client ships the API base and its public app key in its JavaScript bundle. */
async function connect(): Promise<Api> {
  const page = await (await fetch(`${site}/`)).text();
  const path = page.match(/\/static\/js\/main\.[a-f0-9]+\.js/)?.[0];
  if (!path) throw new Error("Club Killers site layout changed (script not found)");
  const bundle = await (await fetch(`${site}${path}`)).text();
  const base = bundle.match(/https:\/\/[a-z.]*clubkillers\.com\/v1\//)?.[0];
  const key = bundle.slice(Math.max(0, bundle.indexOf("Authorization:\"Bearer "))).match(/Authorization:"Bearer ([^"]+)"/)?.[1];
  if (!base || !key) throw new Error("Club Killers site layout changed (API details not found)");
  return new Api(base, key);
}

// The API wraps replies as { status, data, message }; narrow unknown JSON before reading it.
const record = (value: unknown): Record<string, unknown> =>
  typeof value === "object" && value !== null ? value as Record<string, unknown> : {};

function errorText(body: unknown): string {
  const message = record(body).message;
  if (typeof message === "string") return message;
  if (Array.isArray(message) && typeof message[0] === "string") return message[0];
  const inner = record(message).message;
  return typeof inner === "string" ? inner : "request rejected";
}

const safeName = (name: string) => name.replace(/[<>:"/\\|?*\x00-\x1f]/g, "").replace(/\s+/g, " ").trim();

async function run(): Promise<void> {
  const [query, outputDir] = Deno.args;
  const email = Deno.env.get("CK_USERNAME") ?? "";
  const password = Deno.env.get("CK_PASSWORD") ?? "";
  const deviceId = Deno.env.get("CK_DEVICE_ID") ?? "";
  if (!email || !password) throw new Error("Set both Club Killers credential references in Settings");
  if (!query || !outputDir) throw new Error("Missing Club Killers search query or output directory");
  if (/^https?:\/\//i.test(query)) throw new Error("Enter an artist and track title for Club Killers, not a link");

  console.log(JSON.stringify({ stage: "Club Killers login" }));
  const api = await connect();
  const signedIn = await api.call("customer/sign-in.json", { email, password, deviceId });
  const token = record(record(signedIn.body).data).token;
  if (typeof token !== "string" || !token) throw new Error(`login failed: ${errorText(signedIn.body)}`);

  console.log(JSON.stringify({ stage: "Club Killers search" }));
  const songs: Song[] = [];
  for (let page = 1, pages = 1; page <= Math.min(pages, 5); page++) {
    const found = await api.call("song/search.json", {
      page, order: "entity_id", orderDir: "DESC", song_section_id: 1, searchTerm: query, token,
    });
    const data = record(record(found.body).data);
    if (found.status !== 200 || !Array.isArray(data.songs)) throw new Error(`search failed: ${errorText(found.body)}`);
    songs.push(...data.songs);
    pages = Number(record(data.pagination).totalPages) || 1;
  }

  const matches = exactSongs(songs, query);
  if (matches.length === 0) {
    const nearby = songs.slice(0, 5).map((s) => `${s.artist} - ${s.title.trim()}`).join("; ");
    throw new Error(`no exact match for '${query}'${nearby ? `. Results: ${nearby}` : ""}`);
  }
  const usable = matches.filter((song) => pickVersion(song));
  if (usable.length === 0) {
    throw new Error(`no downloadable Intro - Dirty file for '${query}' (${matches.map(describe).join(" | ")})`);
  }
  if (usable.length > 1) {
    throw new Error(`several Club Killers songs match '${query}': ${usable.map((s) => s.title.trim()).join("; ")}`);
  }
  const song = usable[0];
  const version = pickVersion(song)!;

  // Same order as the web client: request the file, then register the download. A refused
  // registration (e.g. the per-file limit) leaves nothing on disk.
  console.log(JSON.stringify({ stage: "Club Killers download" }));
  const dir = TYPE_DIRS[version.version];
  const url = `${fileHost}?url=${`song/songsContainer/${dir}/${version.file_name}`}&file=${encodeURIComponent(version.uploaded_mp3_name)}`;
  // The file script answers "Access denied" unless the request comes from the web client's page.
  const file = await fetch(url, { headers: { Referer: `${site}/` } });
  if (!file.ok || !file.body || !(file.headers.get("content-type") ?? "").startsWith("audio/")) {
    await file.body?.cancel();
    throw new Error(`file request failed (HTTP ${file.status})`);
  }
  const registered = await api.call("download/song-version.json", { version, token });
  if (registered.status !== 200 || record(registered.body).status === "false") {
    await file.body.cancel();
    throw new Error(`download refused: ${errorText(registered.body)}`);
  }

  const disposition = file.headers.get("content-disposition")?.match(/filename="?([^";]+)"?/)?.[1];
  let name = safeName(disposition ? decodeURIComponent(disposition) : version.uploaded_mp3_name) || `${song.artist} - ${song.title}.mp3`;
  let destination = `${outputDir}\\${name}`;
  try {
    await Deno.stat(destination);
    const dot = name.lastIndexOf(".");
    destination = `${outputDir}\\${name.slice(0, dot)}-${crypto.randomUUID().slice(0, 8)}${name.slice(dot)}`;
  } catch { /* name is free */ }
  const target = await Deno.open(destination, { write: true, create: true, truncate: true });
  await file.body.pipeTo(target.writable);
  if ((await Deno.stat(destination)).size === 0) {
    await Deno.remove(destination);
    throw new Error("Club Killers returned an empty audio file");
  }
  console.log(JSON.stringify({ file_path: destination }));
}

if (import.meta.main) {
  try { await run(); }
  catch (error) { console.error(error instanceof Error ? error.message : String(error)); Deno.exitCode = 1; }
}
