import { internal } from "./_generated/api";
import type { Doc, Id } from "./_generated/dataModel";
import type { MutationCtx } from "./_generated/server";

/** What reached us from Slack, before anything decides what it means. */
export type SlackRequest = {
  kind: "mention" | "slashCommand";
  slackUserId: string;
  channelId: string;
  /** The thread the mention was typed in; absent for `/ghostex`, which Slack never ties to a thread. */
  threadTs?: string;
  messageTs?: string;
  /** The text after the bot mention or the slash command, e.g. `cloud fix this`. */
  text: string;
  responseUrl?: string;
  triggerId?: string;
};

export type SlackRouting = { status: "received"; requestId: Id<"slackRequests"> };

/** `cloud …` / `local …` at the start of the text; `null` when the requester named neither. */
export function parseSlackRequestText(text: string): { mode: "cloud" | "local" | null; prompt: string } {
  const withoutMentions = text.replace(/<@[A-Z0-9]+(\|[^>]*)?>/g, " ").trim();
  const match = /^(cloud|local)\b[\s:,-]*/i.exec(withoutMentions);
  if (!match) return { mode: null, prompt: withoutMentions };
  return {
    mode: match[1].toLowerCase() as "cloud" | "local",
    prompt: withoutMentions.slice(match[0].length).trim(),
  };
}

/**
 * Records a Slack request and starts the Slack command flow on it (`slackFlow.ts`): Slack needs an answer within 3 seconds, and the flow calls Slack and Linear, which only actions may do.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: a command reaches the requester's Ghostex, so work always runs on the requester's computer and Claude account. A Slack user not linked to a member yet gets an `unassigned` command that `teams:setIdentity` hands over.
 */
export async function routeSlackRequest(
  ctx: MutationCtx,
  team: Doc<"teams">,
  request: SlackRequest,
): Promise<SlackRouting> {
  const { mode, prompt } = parseSlackRequestText(request.text);
  const now = Date.now();
  const requestId = await ctx.db.insert("slackRequests", {
    teamId: team._id,
    kind: request.kind,
    slackUserId: request.slackUserId,
    channelId: request.channelId,
    threadTs: request.threadTs,
    messageTs: request.messageTs,
    text: request.text,
    mode: mode ?? undefined,
    prompt,
    responseUrl: request.responseUrl,
    status: "received",
    createdAt: now,
    updatedAt: now,
  });
  await ctx.scheduler.runAfter(0, internal.slackFlow.handleRequest, { requestId });
  return { status: "received", requestId };
}
