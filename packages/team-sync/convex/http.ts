import { httpRouter } from "convex/server";
import { internal } from "./_generated/api";
import { httpAction } from "./_generated/server";
import { registerDevMocks } from "./devMocks";
import { verifyLinearRequest, verifySlackRequest } from "./lib/signatures";
import type { SlackRouting } from "./slackRouting";

const http = httpRouter();

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/**
 * Slack Events API (the app's Request URL): `app_mention` starts a request, `message` events keep followed threads up to date.
 */
http.route({
  path: "/slack/events",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    const rawBody = await request.text();
    const check = await verifySlackRequest(request, rawBody, process.env.SLACK_SIGNING_SECRET);
    if (!check.ok) return new Response(check.reason, { status: check.status });
    let envelope: { type?: string; challenge?: string };
    try {
      envelope = JSON.parse(rawBody);
    } catch {
      return new Response("The body is not JSON.", { status: 400 });
    }
    if (envelope.type === "url_verification") {
      return json({ challenge: envelope.challenge ?? "" });
    }
    if (envelope.type !== "event_callback") return new Response(null, { status: 200 });
    const outcome = await ctx.runMutation(internal.slackIntake.receiveEvent, { rawBody });
    return json({ ok: true, ...outcome });
  }),
});

function slashCommandReply(routing: SlackRouting | null): string {
  if (!routing) return "This Slack workspace isn't connected to a Ghostex team.";
  return "Got it. Finding the ticket…";
}

/** The `/ghostex` slash command (top of a channel only; Slack never says which thread a slash command came from). */
http.route({
  path: "/slack/commands",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    const rawBody = await request.text();
    const check = await verifySlackRequest(request, rawBody, process.env.SLACK_SIGNING_SECRET);
    if (!check.ok) return new Response(check.reason, { status: check.status });
    const form = new URLSearchParams(rawBody);
    const slackUserId = form.get("user_id");
    const channelId = form.get("channel_id");
    if (!slackUserId || !channelId) return new Response("Missing user_id or channel_id.", { status: 400 });
    const outcome = await ctx.runMutation(internal.slackIntake.receiveSlashCommand, {
      slackTeamId: form.get("team_id") ?? undefined,
      slackUserId,
      channelId,
      command: form.get("command") ?? "/ghostex",
      text: form.get("text") ?? "",
      responseUrl: form.get("response_url") ?? undefined,
      triggerId: form.get("trigger_id") ?? undefined,
    });
    return json({ response_type: "ephemeral", text: slashCommandReply(outcome.routing) });
  }),
});

/** Button clicks in Ghostex's private notes (Slack app → Interactivity → Request URL). */
http.route({
  path: "/slack/interactivity",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    const rawBody = await request.text();
    const check = await verifySlackRequest(request, rawBody, process.env.SLACK_SIGNING_SECRET);
    if (!check.ok) return new Response(check.reason, { status: check.status });
    const payloadJson = new URLSearchParams(rawBody).get("payload");
    if (!payloadJson) return new Response("Missing payload.", { status: 400 });
    await ctx.runMutation(internal.slackFlowState.receiveInteraction, { payloadJson });
    return new Response(null, { status: 200 });
  }),
});

/** Linear webhooks (Settings → API → Webhooks, URL `https://<deployment>.convex.site/linear/webhook`). */
http.route({
  path: "/linear/webhook",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    const rawBody = await request.text();
    const check = await verifyLinearRequest(request, rawBody, process.env.LINEAR_WEBHOOK_SECRET);
    if (!check.ok) return new Response(check.reason, { status: check.status });
    const deliveryId = request.headers.get("linear-delivery") ?? `delivery:${Date.now()}`;
    const outcome = await ctx.runMutation(internal.linearIntake.receiveWebhook, { deliveryId, rawBody });
    return json({ ok: true, ...outcome });
  }),
});

/**
 * Where an invite link lands when someone opens it in a browser: it says how to join. The code itself is only exchanged by Ghostex (`invites:join`).
 */
http.route({
  path: "/join",
  method: "GET",
  handler: httpAction(async (_ctx, request) => {
    const link = request.url.replace(/[<>&"']/g, "");
    const page = `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Join your team in Ghostex</title><body style="font-family:system-ui,sans-serif;background:#111;color:#eee;max-width:40rem;margin:4rem auto;padding:0 1rem;line-height:1.5"><h1 style="font-size:1.4rem">Join your team in Ghostex</h1><p>Copy this whole link and paste it in Ghostex (Settings → Workspaces → Convex → Join), or run:</p><pre style="white-space:pre-wrap;word-break:break-all;background:#1d1d1d;padding:1rem;border-radius:6px">ghostex team join "${link}"</pre><p>The link works once.</p></body>`;
    return new Response(page, { status: 200, headers: { "content-type": "text/html; charset=utf-8" } });
  }),
});

registerDevMocks(http);

export default http;
