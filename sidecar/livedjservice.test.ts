import { exactMatches } from "./livedjservice.ts";

Deno.test("original track cannot resolve to a remix or edit", () => {
  const tracks = [
    { title: "Drake — Janice STFU (Denero Remix)", id: 1 },
    { title: "Drake — Janice STFU (Intro Edit)", id: 2 },
    { title: "Drake\u00a0— Janice STFU", id: 3 },
  ];
  if (exactMatches(tracks, "Drake Janice STFU")[0]?.id !== 3) {
    throw new Error("original did not resolve to the unmodified track");
  }
  if (exactMatches(tracks, "Drake Janice STFU (Denero Remix)")[0]?.id !== 1) {
    throw new Error("requested remix did not resolve to its own recording");
  }
  if (exactMatches(tracks, "Drake Janice STFU (Clean)").length !== 0) {
    throw new Error("a missing edit must not be substituted");
  }
});

Deno.test("a unique title-only search picks WOTOTO without matching partial titles", () => {
  const tracks = [
    { title: "Boombox Cartel, A-Par, & Mc Bin Laden\u00a0— Wototo", id: 1 },
    { title: "ODUMODUBLVCK — WOTOWOTO SEASONING", id: 2 },
  ];
  const matches = exactMatches(tracks, "WOTOTO");
  if (matches.length !== 1 || matches[0].id !== 1) throw new Error("selected a different song");
  const ambiguous = exactMatches([...tracks, { title: "Another Artist — Wototo", id: 3 }], "Wototo");
  if (ambiguous.length !== 2) throw new Error("ambiguous titles must not be collapsed to one");
});
