import { v } from "convex/values";
import { internalMutation } from "./_generated/server";

/**
 * Records one signed Linear webhook delivery, deduplicated by `Linear-Delivery`.
 *
 * CDXC:TeamSync 2026-10-09 WHY:
 * Only stored for now; reacting to issue and comment changes (live ticket rows on the Work page, commands for an assignee) plugs in here.
 */
export const receiveWebhook = internalMutation({
  args: { deliveryId: v.string(), rawBody: v.string() },
  handler: async (ctx, args) => {
    const seen = await ctx.db
      .query("linearEvents")
      .withIndex("by_delivery", (q) => q.eq("deliveryId", args.deliveryId))
      .first();
    if (seen) return { duplicate: true };
    const payload = JSON.parse(args.rawBody) as Record<string, unknown>;
    const teams = await ctx.db.query("teams").take(2);
    await ctx.db.insert("linearEvents", {
      teamId: teams.length === 1 ? teams[0]._id : undefined,
      deliveryId: args.deliveryId,
      eventType: String(payload.type ?? "unknown"),
      action: String(payload.action ?? "unknown"),
      payloadJson: args.rawBody,
      receivedAt: Date.now(),
    });
    return { duplicate: false };
  },
});
