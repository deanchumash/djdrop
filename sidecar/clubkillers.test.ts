import { exactSongs, pickVersion, type Song, type SongVersion } from "./clubkillers.ts";

const version = (version_label: string, used = 0, version = 4): SongVersion => ({
  version_label, version, file_name: `${version_label}.mp3`, uploaded_mp3_name: `${version_label}.mp3`,
  download: [0, 1, 2].map((slot) => slot < used),
});
const song = (title: string, artist: string, ...versions: SongVersion[]): Song => ({ title, artist, version: versions });

Deno.test("an accented artist and a plain title resolve to the original, not a remix", () => {
  const songs = [
    song("Nicole Kidman (MOONLGHT Remix)", "ADÉLA"),
    song("Nicole Kidman", "ADÉLA"),
    song("Boys", "ADELA"),
  ];
  const found = exactSongs(songs, "ADELA Nicole Kidman");
  if (found.length !== 1 || found[0].title !== "Nicole Kidman") throw new Error("picked the wrong recording");
  if (exactSongs(songs, "Nicole Kidman ADÉLA").length !== 1) throw new Error("word order must not matter");
  if (exactSongs(songs, "Nicole Kidman (HILLS Remix)").length !== 0) throw new Error("a missing remix must not be substituted");
});

Deno.test("the CK dirty intro wins, any other dirty intro is accepted, nothing else is", () => {
  const ck = song("A", "X", version("Intro - Dirty"), version("CK Intro - Dirty"), version("Intro - Clean", 0, 3));
  if (pickVersion(ck)?.version_label !== "CK Intro - Dirty") throw new Error("CK Intro - Dirty must be preferred");
  if (pickVersion(song("A", "X", version("Intro - Clean", 0, 3), version("Intro - Dirty")))?.version_label !== "Intro - Dirty") {
    throw new Error("a plain dirty intro must be accepted");
  }
  if (pickVersion(song("A", "X", version("CK Cut Dirty", 0, 6), version("Extended Dirty", 0, 6), version("Clean", 0, 5)))) {
    throw new Error("only dirty intros may be chosen");
  }
});

Deno.test("a file at its three-download limit is never chosen", () => {
  if (pickVersion(song("A", "X", version("CK Intro - Dirty", 3), version("Intro - Dirty", 2)))?.version_label !== "Intro - Dirty") {
    throw new Error("must fall through to the file that still has downloads left");
  }
  if (pickVersion(song("A", "X", version("CK Intro - Dirty", 3)))) throw new Error("limit reached must yield nothing");
});
