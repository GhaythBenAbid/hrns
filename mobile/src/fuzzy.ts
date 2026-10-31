// fzf-style fuzzy matching (its v1 algorithm and scoring constants), for the project finder.
//
// A term matches when its characters appear in order. The match is the shortest window
// found by a forward scan then a backward scan; it scores points per character, with bonuses
// for word starts and consecutive runs and penalties for gaps. Space-separated terms must all
// match (any order). Smart case: lowercase terms ignore case, any capital makes the term exact.

const SCORE_MATCH = 16;
const GAP_START = -3;
const GAP_EXTENSION = -1;
const BONUS_BOUNDARY = SCORE_MATCH / 2; // after a separator or at the start
const BONUS_NON_WORD = SCORE_MATCH / 2;
const BONUS_CAMEL = BONUS_BOUNDARY + GAP_EXTENSION;
const BONUS_CONSECUTIVE = -(GAP_START + GAP_EXTENSION);
const BONUS_FIRST_CHAR_MULTIPLIER = 2;
const BONUS_PATH_SEP = BONUS_BOUNDARY + 1; // "/" boundaries count a little more, as in fzf's path scheme

export type Match = { score: number; positions: number[] };

type Class = "lower" | "upper" | "digit" | "sep" | "other";

function classOf(c: string | undefined): Class {
  if (c === undefined) return "sep";
  if (c === "/" ) return "sep";
  if (c >= "a" && c <= "z") return "lower";
  if (c >= "A" && c <= "Z") return "upper";
  if (c >= "0" && c <= "9") return "digit";
  if (" -_.:".includes(c)) return "sep";
  return "other";
}

function bonusAt(text: string, i: number): number {
  const prev = i === 0 ? undefined : text[i - 1];
  const cur = classOf(text[i]);
  const pc = classOf(prev);
  if (cur === "sep" || cur === "other") return BONUS_NON_WORD;
  if (prev === undefined || prev === "/") return BONUS_PATH_SEP;
  if (pc === "sep" || pc === "other") return BONUS_BOUNDARY;
  if (pc === "lower" && cur === "upper") return BONUS_CAMEL;
  if (pc !== "digit" && cur === "digit") return BONUS_CAMEL;
  return 0;
}

function matchTerm(term: string, text: string): Match | null {
  const exact = term !== term.toLowerCase();
  const hay = exact ? text : text.toLowerCase();
  const n = term.length;

  // Forward: earliest point where the whole term has appeared in order.
  let ti = 0;
  let end = -1;
  for (let i = 0; i < hay.length; i++) {
    if (hay[i] === term[ti] && ++ti === n) {
      end = i;
      break;
    }
  }
  if (end < 0) return null;

  // Backward from there: the latest start, giving the tightest window.
  let start = end;
  ti = n - 1;
  for (let i = end; i >= 0; i--) {
    if (hay[i] === term[ti] && --ti < 0) {
      start = i;
      break;
    }
  }

  // Score the window, matching greedily from the start.
  const positions: number[] = [];
  let score = 0;
  let inGap = false;
  let consecutive = 0;
  let firstBonus = 0;
  ti = 0;
  for (let i = start; i <= end; i++) {
    if (ti < n && hay[i] === term[ti]) {
      let bonus = bonusAt(text, i);
      if (consecutive === 0) firstBonus = bonus;
      else {
        // A run keeps the bonus of its first character, like fzf.
        if (bonus >= BONUS_BOUNDARY && bonus > firstBonus) firstBonus = bonus;
        bonus = Math.max(bonus, firstBonus, BONUS_CONSECUTIVE);
      }
      score += SCORE_MATCH + (ti === 0 ? bonus * BONUS_FIRST_CHAR_MULTIPLIER : bonus);
      positions.push(i);
      consecutive++;
      inGap = false;
      ti++;
    } else {
      score += inGap ? GAP_EXTENSION : GAP_START;
      inGap = true;
      consecutive = 0;
      firstBonus = 0;
    }
  }
  return { score, positions };
}

/**
 * Match a query against a path. Each term is tried on the folder name first (what people
 * usually type) and the full path second; the better score wins.
 */
export function fuzzy(query: string, path: string): Match | null {
  const terms = query.trim().split(/\s+/).filter(Boolean);
  if (!terms.length) return { score: 0, positions: [] };
  const base = path.lastIndexOf("/") + 1;
  const name = path.slice(base);
  let score = 0;
  const positions = new Set<number>();
  for (const term of terms) {
    const full = matchTerm(term, path);
    if (!full) return null;
    const short = matchTerm(term, name);
    // Matching inside the folder name is worth more than spreading across parent folders.
    const best = short && short.score + 2 * SCORE_MATCH >= full.score
      ? { score: short.score + 2 * SCORE_MATCH, positions: short.positions.map((p) => p + base) }
      : full;
    score += best.score;
    for (const p of best.positions) positions.add(p);
  }
  return { score, positions: [...positions].sort((a, b) => a - b) };
}
