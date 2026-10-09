import { ConvexError, v } from "convex/values";
import { internal } from "./_generated/api";
import type { Id } from "./_generated/dataModel";
import { action, internalQuery } from "./_generated/server";
import { normalizeTicket, requireMember } from "./lib/auth";
import { postMessage } from "./lib/slackApi";
import type { PostTarget } from "./slackFlowState";

const MAX_TEXT_CHARS = 3_500;

export const memberOfToken = internalQuery({
  args: { memberToken: v.string() },
  handler: async (ctx, args): Promise<Id<"members">> => (await requireMember(ctx, args.memberToken))._id,
});

/**
 * `ghostex slack post --session <ref> "<text>"`: a session posts to its ticket's working thread, as "Claude · SPX-1234". With `final`, the text also goes once to every source thread the ticket's requests came from.
 *
 * CDXC:TeamSync 2026-10-09 DECISION:
 * User: the agent decides when a milestone happened (numbered 1a, 1b, 2a… as the team instructions say) and posts it through a `ghostex slack post` command Ghostex gives it; the source thread gets only the final result (PR, QC package version, video).
 */
export const postForSession = action({
  args: {
    memberToken: v.string(),
    sessionId: v.optional(v.string()),
    tickets: v.array(v.string()),
    text: v.string(),
    final: v.optional(v.boolean()),
  },
  handler: async (ctx, args): Promise<{ ticket: string; posted: { channelId: string; ts: string }[] }> => {
    const text = args.text.trim();
    if (!text) throw new ConvexError("Pass the text to post.");
    if (text.length > MAX_TEXT_CHARS) throw new ConvexError(`Keep a post under ${MAX_TEXT_CHARS} characters.`);
    const memberId: Id<"members"> = await ctx.runQuery(internal.slackPost.memberOfToken, { memberToken: args.memberToken });
    const target: PostTarget | null = await ctx.runQuery(internal.slackFlowState.resolvePostTarget, {
      memberId,
      sessionId: args.sessionId,
      tickets: args.tickets.filter((ticket) => ticket.trim()).map(normalizeTicket),
    });
    if (!target) {
      throw new ConvexError("This session's ticket has no Slack working thread. Working threads are opened by `@Ghostex` in Slack.");
    }
    const username = `Claude · ${target.ticket}`;
    const posted = [await postMessage({ channel: target.workingThread.channelId, threadTs: target.workingThread.threadTs, text, username })];
    if (args.final) {
      for (const source of target.sourceThreads) {
        posted.push(await postMessage({ channel: source.channelId, threadTs: source.threadTs, text, username }));
      }
    }
    return { ticket: target.ticket, posted: posted.map((post) => ({ channelId: post.channel, ts: post.ts })) };
  },
});
