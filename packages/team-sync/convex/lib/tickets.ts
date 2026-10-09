/**
 * Finding tickets in Slack text: Linear IDs (only the team's real keys), Linear links, and GitHub issue and PR links.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: Linear IDs count only with the team's real keys, so "UTF-8" never matches. A ticket is stored as `SPX-1234` (Linear) or `owner/repo#12` (a GitHub issue or PR).
 */

export type TicketRef =
  | { kind: "linear"; key: string }
  | { kind: "github"; key: string; repo: string; number: number; pullRequest: boolean };

const LINEAR_LINK = /https?:\/\/linear\.app\/[^/\s>|]+\/issue\/([A-Za-z][A-Za-z0-9]*-\d+)/g;
const GITHUB_LINK = /https?:\/\/github\.com\/([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)\/(issues|pull)\/(\d+)/g;
const LINEAR_ID = /(?<![A-Za-z0-9])([A-Za-z][A-Za-z0-9]{0,9})-(\d{1,7})(?![A-Za-z0-9])/g;

export function githubTicketKey(repo: string, number: number): string {
  return `${repo.toLowerCase()}#${number}`;
}

/** Every distinct ticket in `text`, in the order they first appear. */
export function findTickets(text: string, linearTeamKeys: string[]): TicketRef[] {
  const found = new Map<string, { at: number; ticket: TicketRef }>();
  const add = (at: number, ticket: TicketRef) => {
    if (!found.has(ticket.key)) found.set(ticket.key, { at, ticket });
  };
  for (const match of text.matchAll(LINEAR_LINK)) {
    add(match.index ?? 0, { kind: "linear", key: match[1].toUpperCase() });
  }
  for (const match of text.matchAll(GITHUB_LINK)) {
    const repo = `${match[1]}/${match[2]}`;
    const number = Number(match[4]);
    add(match.index ?? 0, {
      kind: "github",
      key: githubTicketKey(repo, number),
      repo: repo.toLowerCase(),
      number,
      pullRequest: match[3] === "pull",
    });
  }
  const keys = new Set(linearTeamKeys.map((key) => key.toUpperCase()));
  // GitHub links carry numbers that look like nothing Linear uses, but a URL path can hold an ID-like word: skip what the links above already covered.
  const withoutGithubLinks = text.replace(GITHUB_LINK, (link) => " ".repeat(link.length));
  for (const match of withoutGithubLinks.matchAll(LINEAR_ID)) {
    if (keys.has(match[1].toUpperCase())) {
      add(match.index ?? 0, { kind: "linear", key: `${match[1].toUpperCase()}-${Number(match[2])}` });
    }
  }
  return [...found.values()].sort((left, right) => left.at - right.at).map((entry) => entry.ticket);
}

/** The ticket a stored key names. */
export function parseTicketKey(key: string): TicketRef {
  const github = /^([^/\s]+\/[^#\s]+)#(\d+)$/.exec(key);
  if (github) {
    return { kind: "github", key, repo: github[1], number: Number(github[2]), pullRequest: false };
  }
  return { kind: "linear", key };
}

/** A ticket title from free text: the first sentence or line, without Slack markup, at most 80 characters. */
export function titleFromText(text: string): string {
  const firstLine = text
    .split(/\n+/)
    .map((line) => line.trim())
    .find((line) => line.length > 0) ?? "";
  const sentence = /^(.{12,}?[.!?])(\s|$)/.exec(firstLine)?.[1] ?? firstLine;
  const clean = sentence.replace(/\s+/g, " ").trim();
  return clean.length <= 80 ? clean : `${clean.slice(0, 79).trimEnd()}…`;
}
