import { v } from "convex/values";
import type { Doc, Id } from "./_generated/dataModel";
import type { MutationCtx } from "./_generated/server";
import { internalMutation } from "./_generated/server";
import { routeSlackRequest, type SlackRouting } from "./slackRouting";
import { findThread, upsertThread } from "./slackThreads";

type SlackFile = { id?: string; name?: string; mimetype?: string; url_private?: string; permalink?: string };
type SlackMessage = {
  ts?: string;
  thread_ts?: string;
  user?: string;
  bot_id?: string;
  username?: string;
  text?: string;
  files?: SlackFile[];
  edited?: { ts?: string };
};

/**
 * The team a Slack delivery belongs to: the one bound to its Slack workspace, else the deployment's only team (which then gets bound to it).
 */
async function teamForSlack(ctx: MutationCtx, slackTeamId: string | undefined): Promise<Doc<"teams"> | null> {
  if (slackTeamId) {
    const bound = await ctx.db
      .query("teams")
      .withIndex("by_slack_team", (q) => q.eq("slackTeamId", slackTeamId))
      .first();
    if (bound) return bound;
  }
  const teams = await ctx.db.query("teams").take(2);
  if (teams.length !== 1) return null;
  const [team] = teams;
  if (slackTeamId && !team.slackTeamId) {
    await ctx.db.patch(team._id, { slackTeamId });
    return { ...team, slackTeamId };
  }
  return team.slackTeamId === undefined || team.slackTeamId === slackTeamId ? team : null;
}

function slackTsToMs(ts: string | undefined): number {
  const seconds = Number(ts);
  return Number.isFinite(seconds) ? Math.round(seconds * 1000) : Date.now();
}

function files(message: SlackMessage): Doc<"slackMessages">["files"] {
  return (message.files ?? [])
    .filter((file) => typeof file.id === "string")
    .map((file) => ({
      id: file.id!,
      name: file.name,
      mimetype: file.mimetype,
      urlPrivate: file.url_private,
      permalink: file.permalink,
    }));
}

export async function upsertMessage(
  ctx: MutationCtx,
  team: Doc<"teams">,
  thread: Doc<"slackThreads">,
  message: SlackMessage,
): Promise<void> {
  if (!message.ts) return;
  const postedAt = slackTsToMs(message.ts);
  const existing = await ctx.db
    .query("slackMessages")
    .withIndex("by_thread_ts", (q) => q.eq("threadId", thread._id).eq("ts", message.ts!))
    .unique();
  const fields = {
    userId: message.user,
    botId: message.bot_id,
    authorName: message.username,
    text: message.text ?? "",
    files: files(message),
  };
  if (existing) {
    await ctx.db.patch(existing._id, {
      ...fields,
      editedAt: message.edited?.ts ? slackTsToMs(message.edited.ts) : existing.editedAt,
    });
  } else {
    await ctx.db.insert("slackMessages", { teamId: team._id, threadId: thread._id, ts: message.ts, postedAt, ...fields });
  }
  if ((thread.lastMessageAt ?? 0) < postedAt) {
    await ctx.db.patch(thread._id, { lastMessageAt: postedAt, updatedAt: Date.now() });
  }
}

/** Stores a message, edit or deletion when it belongs to a thread Ghostex already follows. */
async function recordThreadMessage(ctx: MutationCtx, team: Doc<"teams">, event: Record<string, any>) {
  const channelId: string | undefined = event.channel;
  if (!channelId) return;
  if (event.subtype === "message_deleted") {
    const threadTs = event.previous_message?.thread_ts;
    const thread = threadTs ? await findThread(ctx, team._id, channelId, threadTs) : null;
    if (!thread || !event.deleted_ts) return;
    const message = await ctx.db
      .query("slackMessages")
      .withIndex("by_thread_ts", (q) => q.eq("threadId", thread._id).eq("ts", event.deleted_ts))
      .unique();
    if (message) await ctx.db.patch(message._id, { deletedAt: Date.now() });
    return;
  }
  const message: SlackMessage = event.subtype === "message_changed" ? (event.message ?? {}) : event;
  const threadTs = message.thread_ts ?? message.ts;
  if (!threadTs) return;
  const thread = await findThread(ctx, team._id, channelId, threadTs);
  if (thread) await upsertMessage(ctx, team, thread, message);
}

/** Records one signed Events API delivery. Returns what happened, for logs and the HTTP reply. */
export const receiveEvent = internalMutation({
  args: { rawBody: v.string() },
  handler: async (ctx, args): Promise<{ duplicate: boolean; teamId: Id<"teams"> | null; routing: SlackRouting | null }> => {
    const envelope = JSON.parse(args.rawBody) as { event_id?: string; team_id?: string; event?: Record<string, any> };
    const eventId = envelope.event_id ?? `event:${Date.now()}`;
    const seen = await ctx.db
      .query("slackEvents")
      .withIndex("by_event_id", (q) => q.eq("eventId", eventId))
      .first();
    if (seen) return { duplicate: true, teamId: seen.teamId ?? null, routing: null };
    const team = await teamForSlack(ctx, envelope.team_id);
    const event = envelope.event ?? {};
    await ctx.db.insert("slackEvents", {
      teamId: team?._id,
      eventId,
      eventType: String(event.type ?? "unknown"),
      payloadJson: args.rawBody,
      receivedAt: Date.now(),
    });
    if (!team) return { duplicate: false, teamId: null, routing: null };

    if (event.type === "app_mention" && event.channel && event.user && event.ts) {
      const threadTs: string = event.thread_ts ?? event.ts;
      const thread = await upsertThread(ctx, team._id, { channelId: event.channel, threadTs });
      await upsertMessage(ctx, team, thread, event);
      const routing = await routeSlackRequest(ctx, team, {
        kind: "mention",
        slackUserId: event.user,
        channelId: event.channel,
        threadTs,
        messageTs: event.ts,
        text: event.text ?? "",
      });
      return { duplicate: false, teamId: team._id, routing };
    }
    if (event.type === "message") {
      await recordThreadMessage(ctx, team, event);
    }
    return { duplicate: false, teamId: team._id, routing: null };
  },
});

/** Records one signed `/ghostex` slash command. */
export const receiveSlashCommand = internalMutation({
  args: {
    slackTeamId: v.optional(v.string()),
    slackUserId: v.string(),
    channelId: v.string(),
    command: v.string(),
    text: v.string(),
    responseUrl: v.optional(v.string()),
    triggerId: v.optional(v.string()),
  },
  handler: async (ctx, args): Promise<{ teamId: Id<"teams"> | null; routing: SlackRouting | null }> => {
    const team = await teamForSlack(ctx, args.slackTeamId);
    await ctx.db.insert("slackEvents", {
      teamId: team?._id,
      eventId: `command:${args.triggerId ?? `${args.channelId}:${Date.now()}`}`,
      eventType: "slash_command",
      payloadJson: JSON.stringify(args),
      receivedAt: Date.now(),
    });
    if (!team) return { teamId: null, routing: null };
    const routing = await routeSlackRequest(ctx, team, {
      kind: "slashCommand",
      slackUserId: args.slackUserId,
      channelId: args.channelId,
      text: args.text,
      responseUrl: args.responseUrl,
      triggerId: args.triggerId,
    });
    return { teamId: team._id, routing };
  },
});
