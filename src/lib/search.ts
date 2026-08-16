/**
 * Spelling-tolerant matching for lists written in US English.
 *
 * Fooocus's style names use US spellings — "Watercolor", "Color Field
 * Painting", "Mk Coloring Book" — so a search for "colour" would otherwise
 * find nothing at all.
 *
 * The substitutions are applied to both the query and the candidate, so they
 * only ever need to agree with each other, not with real English. That makes
 * over-eager rules harmless: "noise" and "noize" both normalise the same way,
 * and a false match in a search filter costs nothing.
 */
function normalise(text: string): string {
  return text
    .toLowerCase()
    .replace(/colour/g, "color")
    .replace(/grey/g, "gray")
    .replace(/isation/g, "ization")
    .replace(/ise\b/g, "ize")
    .replace(/yse\b/g, "yze")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

/** True when `candidate` matches `query`, ignoring spelling and punctuation. */
export function matches(candidate: string, query: string): boolean {
  const q = normalise(query);
  if (!q) return true;

  const target = normalise(candidate);
  // Every word must appear, so "color field" matches "Color Field Painting"
  // regardless of order or extra words between them.
  return q.split(" ").every((word) => target.includes(word));
}
