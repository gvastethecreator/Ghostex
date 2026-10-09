/**
 * The Slack Web API calls the Slack command flow makes, as the team's bot (`SLACK_BOT_TOKEN`).
 *
 * CDXC:TeamSync 2026-10-09 WHY:
 * `SLACK_API_BASE_URL` (default `https://slack.com/api`) points every call somewhere else, so a test deployment talks to a mock and never posts into a real Slack workspace. `response_url` replies go to the URL Slack put in the payload, which a signed test payload sets to the same mock.
 */

export type SlackApiResult = { ok: boolean; error?: string; [key: string]: unknown };

export class SlackApiError extends Error {
  constructor(
    readonly method: string,
    readonly slackError: string,
  ) {
    super(`Slack ${method} failed: ${slackError}`);
  }
}

export function slackApiBaseUrl(): string {
  return (process.env.SLACK_API_BASE_URL ?? "https://slack.com/api").replace(/\/+$/, "");
}

function botToken(): string {
  const token = process.env.SLACK_BOT_TOKEN;
  if (!token) {
    throw new Error("SLACK_BOT_TOKEN is not set on this deployment. Run `ghostex team slack-connect`.");
  }
  return token;
}

/** One Web API method with a JSON body. Throws a SlackApiError when Slack answers `ok: false`. */
export async function slackApi(method: string, body: Record<string, unknown>): Promise<SlackApiResult> {
  const response = await fetch(`${slackApiBaseUrl()}/${method}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${botToken()}`,
      "content-type": "application/json; charset=utf-8",
    },
    body: JSON.stringify(body),
  });
  const result = (await response.json().catch(() => ({ ok: false, error: `http_${response.status}` }))) as SlackApiResult;
  if (!result.ok) throw new SlackApiError(method, String(result.error ?? "unknown_error"));
  return result;
}

/** A read method (`conversations.replies`, `users.info`, `chat.getPermalink`) with query parameters. */
export async function slackApiGet(method: string, params: Record<string, string>): Promise<SlackApiResult> {
  const query = new URLSearchParams(params).toString();
  const response = await fetch(`${slackApiBaseUrl()}/${method}?${query}`, {
    headers: { authorization: `Bearer ${botToken()}` },
  });
  const result = (await response.json().catch(() => ({ ok: false, error: `http_${response.status}` }))) as SlackApiResult;
  if (!result.ok) throw new SlackApiError(method, String(result.error ?? "unknown_error"));
  return result;
}

export async function postMessage(fields: {
  channel: string;
  text: string;
  threadTs?: string;
  username?: string;
  blocks?: unknown[];
}): Promise<{ channel: string; ts: string }> {
  const result = await slackApi("chat.postMessage", {
    channel: fields.channel,
    text: fields.text,
    thread_ts: fields.threadTs,
    username: fields.username,
    blocks: fields.blocks,
    unfurl_links: false,
    unfurl_media: false,
  });
  return { channel: String(result.channel ?? fields.channel), ts: String(result.ts ?? "") };
}

/**
 * A message only the requester sees. `responseUrl` (a slash command or a button click) works in channels the bot is not in; otherwise `chat.postEphemeral`.
 */
export async function postPrivateNote(fields: {
  channel: string;
  user: string;
  text: string;
  threadTs?: string;
  blocks?: unknown[];
  responseUrl?: string;
  replaceOriginal?: boolean;
}): Promise<void> {
  if (fields.responseUrl) {
    const response = await fetch(fields.responseUrl, {
      method: "POST",
      headers: { "content-type": "application/json; charset=utf-8" },
      body: JSON.stringify({
        response_type: "ephemeral",
        replace_original: fields.replaceOriginal ?? false,
        text: fields.text,
        blocks: fields.blocks,
      }),
    });
    if (!response.ok) throw new SlackApiError("response_url", `http_${response.status}`);
    return;
  }
  await slackApi("chat.postEphemeral", {
    channel: fields.channel,
    user: fields.user,
    text: fields.text,
    thread_ts: fields.threadTs,
    blocks: fields.blocks,
  });
}

/** Replaces the text of a message the bot posted. */
export async function updateMessage(channel: string, ts: string, text: string): Promise<void> {
  await slackApi("chat.update", { channel, ts, text });
}

export async function addReaction(channel: string, ts: string, name: string): Promise<void> {
  try {
    await slackApi("reactions.add", { channel, timestamp: ts, name });
  } catch (error) {
    // Reacting twice is not a failure.
    if (!(error instanceof SlackApiError && error.slackError === "already_reacted")) throw error;
  }
}

export async function permalink(channel: string, messageTs: string): Promise<string | undefined> {
  const result = await slackApiGet("chat.getPermalink", { channel, message_ts: messageTs });
  return typeof result.permalink === "string" ? result.permalink : undefined;
}

export type SlackApiFile = { id?: string; name?: string; mimetype?: string; url_private?: string; permalink?: string; size?: number };
export type SlackApiAttachment = { title?: string; title_link?: string; text?: string; fallback?: string; from_url?: string; original_url?: string; service_name?: string };
export type SlackApiMessage = {
  ts?: string;
  thread_ts?: string;
  user?: string;
  bot_id?: string;
  username?: string;
  text?: string;
  files?: SlackApiFile[];
  attachments?: SlackApiAttachment[];
  edited?: { ts?: string };
};

/** Every message of a thread, oldest first (the parent included), following Slack's pages. */
export async function threadReplies(channel: string, threadTs: string): Promise<SlackApiMessage[]> {
  const messages: SlackApiMessage[] = [];
  let cursor: string | undefined;
  for (let page = 0; page < 5; page += 1) {
    const params: Record<string, string> = { channel, ts: threadTs, limit: "200" };
    if (cursor) params.cursor = cursor;
    const result = await slackApiGet("conversations.replies", params);
    messages.push(...((result.messages as SlackApiMessage[] | undefined) ?? []));
    cursor = (result.response_metadata as { next_cursor?: string } | undefined)?.next_cursor || undefined;
    if (!cursor) break;
  }
  return messages;
}

/** Display names by Slack user id, asked once per user. */
export async function userNames(userIds: Iterable<string>): Promise<Map<string, string>> {
  const names = new Map<string, string>();
  for (const id of new Set(userIds)) {
    try {
      const result = await slackApiGet("users.info", { user: id });
      const user = result.user as { real_name?: string; name?: string; profile?: { display_name?: string; real_name?: string } } | undefined;
      const name = user?.profile?.display_name || user?.profile?.real_name || user?.real_name || user?.name;
      if (name) names.set(id, name);
    } catch {
      // An unknown user is shown by id.
    }
  }
  return names;
}

/** Downloads a private Slack file with the bot token (`files:read`). */
export async function downloadFile(urlPrivate: string): Promise<Blob> {
  const response = await fetch(urlPrivate, { headers: { authorization: `Bearer ${botToken()}` } });
  if (!response.ok) throw new SlackApiError("files.download", `http_${response.status}`);
  return await response.blob();
}

/** Slack's markup as plain text: user and channel mentions, `<url|label>` links. */
export function plainSlackText(text: string, names: Map<string, string> = new Map()): string {
  return text
    .replace(/<@([A-Z0-9]+)(\|[^>]*)?>/g, (_, id: string) => `@${names.get(id) ?? id}`)
    .replace(/<#([A-Z0-9]+)\|([^>]*)>/g, (_, _id: string, name: string) => `#${name}`)
    .replace(/<(https?:[^>|]+)\|([^>]+)>/g, (_, url: string, label: string) => `${label} (${url})`)
    .replace(/<(https?:[^>]+)>/g, "$1")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&");
}

/** Text Slack shows literally (Slack's three escapes). */
export function escapeSlack(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
