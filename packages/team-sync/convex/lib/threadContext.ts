import { plainSlackText, type SlackApiMessage } from "./slackApi";

/** What step 1 ("read the thread") hands the rest of the flow. */
export type ThreadContext = {
  /** Every message, reply, file and link preview, as text Claude reads. */
  transcript: string;
  /** All text to search for tickets: messages, link previews and the prompt. */
  searchText: string;
  /** The thread's first message, without markup (a new ticket's title comes from it). */
  rootText: string;
  rootAuthor: string | null;
  /** Images in the thread, newest last. */
  images: { name: string; mimetype: string; urlPrivate: string; size: number | null; messageTs: string }[];
  userIds: string[];
};

const TRANSCRIPT_MAX_CHARS = 30_000;

function when(ts: string | undefined): string {
  const seconds = Number(ts);
  if (!Number.isFinite(seconds)) return "";
  return new Date(seconds * 1000).toISOString().slice(0, 16).replace("T", " ") + " UTC";
}

export function messageUserIds(messages: SlackApiMessage[]): string[] {
  const ids = new Set<string>();
  for (const message of messages) {
    if (message.user) ids.add(message.user);
    for (const match of (message.text ?? "").matchAll(/<@([A-Z0-9]+)/g)) ids.add(match[1]);
  }
  return [...ids];
}

export function buildThreadContext(messages: SlackApiMessage[], names: Map<string, string>, prompt: string): ThreadContext {
  const lines: string[] = [];
  const search: string[] = [prompt];
  const images: ThreadContext["images"] = [];
  for (const message of messages) {
    const author = message.user ? (names.get(message.user) ?? message.user) : (message.username ?? "a bot");
    const raw = message.text ?? "";
    search.push(raw);
    lines.push(`${author} (${when(message.ts)}):`);
    const text = plainSlackText(raw, names).trim();
    if (text) lines.push(...text.split("\n").map((line) => `  ${line}`));
    for (const file of message.files ?? []) {
      lines.push(`  [file] ${file.name ?? file.id ?? "file"}${file.mimetype ? ` (${file.mimetype})` : ""}`);
      if (file.url_private && file.mimetype?.startsWith("image/")) {
        images.push({
          name: file.name ?? `${file.id ?? "image"}.png`,
          mimetype: file.mimetype,
          urlPrivate: file.url_private,
          size: file.size ?? null,
          messageTs: message.ts ?? "",
        });
      }
    }
    for (const attachment of message.attachments ?? []) {
      const url = attachment.title_link ?? attachment.from_url ?? attachment.original_url ?? "";
      const title = attachment.title ?? attachment.service_name ?? "";
      const body = (attachment.text ?? attachment.fallback ?? "").trim();
      search.push(url, title, body);
      lines.push(`  [link preview] ${[title, url].filter(Boolean).join(" — ")}`);
      if (body) lines.push(...body.split("\n").slice(0, 8).map((line) => `    ${line}`));
    }
  }
  let transcript = lines.join("\n");
  if (transcript.length > TRANSCRIPT_MAX_CHARS) {
    transcript = `…(earlier messages cut)\n${transcript.slice(transcript.length - TRANSCRIPT_MAX_CHARS)}`;
  }
  const root = messages[0];
  return {
    transcript,
    searchText: search.join("\n"),
    rootText: plainSlackText((root?.text ?? "").replace(/<@[A-Z0-9]+(\|[^>]*)?>/g, ""), names).trim(),
    rootAuthor: root?.user ? (names.get(root.user) ?? root.user) : null,
    images,
    userIds: messageUserIds(messages),
  };
}
