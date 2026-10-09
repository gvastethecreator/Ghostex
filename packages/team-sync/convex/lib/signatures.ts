import { hex } from "./auth";

/** Slack rejects replays older than five minutes; so do we. */
const SLACK_MAX_SKEW_SECONDS = 5 * 60;
/** Linear's guidance: refuse a webhook whose `webhookTimestamp` is more than a minute off. */
const LINEAR_MAX_SKEW_MS = 60 * 1000;

async function hmacSha256Hex(secret: string, message: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(message));
  return hex(new Uint8Array(signature));
}

function constantTimeEqual(left: string, right: string): boolean {
  if (left.length !== right.length) return false;
  let difference = 0;
  for (let index = 0; index < left.length; index += 1) {
    difference |= left.charCodeAt(index) ^ right.charCodeAt(index);
  }
  return difference === 0;
}

export type SignatureCheck = { ok: true } | { ok: false; status: number; reason: string };

/**
 * Slack's request signing: `v0=` + HMAC-SHA256(signing secret, `v0:<timestamp>:<raw body>`).
 * https://api.slack.com/authentication/verifying-requests-from-slack
 */
export async function verifySlackRequest(
  request: Request,
  rawBody: string,
  signingSecret: string | undefined,
): Promise<SignatureCheck> {
  if (!signingSecret) {
    return { ok: false, status: 503, reason: "SLACK_SIGNING_SECRET is not set on this deployment." };
  }
  const timestamp = request.headers.get("x-slack-request-timestamp") ?? "";
  const signature = request.headers.get("x-slack-signature") ?? "";
  const seconds = Number(timestamp);
  if (!Number.isFinite(seconds) || Math.abs(Date.now() / 1000 - seconds) > SLACK_MAX_SKEW_SECONDS) {
    return { ok: false, status: 401, reason: "Stale or missing Slack timestamp." };
  }
  const expected = `v0=${await hmacSha256Hex(signingSecret, `v0:${timestamp}:${rawBody}`)}`;
  return constantTimeEqual(expected, signature)
    ? { ok: true }
    : { ok: false, status: 401, reason: "Bad Slack signature." };
}

/**
 * Linear's webhook signing: hex HMAC-SHA256(webhook secret, raw body) in `Linear-Signature`, plus a fresh `webhookTimestamp` in the body.
 * https://linear.app/developers/webhooks#securing-webhooks
 */
export async function verifyLinearRequest(
  request: Request,
  rawBody: string,
  webhookSecret: string | undefined,
): Promise<SignatureCheck> {
  if (!webhookSecret) {
    return { ok: false, status: 503, reason: "LINEAR_WEBHOOK_SECRET is not set on this deployment." };
  }
  const signature = (request.headers.get("linear-signature") ?? "").toLowerCase();
  const expected = await hmacSha256Hex(webhookSecret, rawBody);
  if (!constantTimeEqual(expected, signature)) {
    return { ok: false, status: 401, reason: "Bad Linear signature." };
  }
  let webhookTimestamp: unknown;
  try {
    webhookTimestamp = (JSON.parse(rawBody) as { webhookTimestamp?: unknown }).webhookTimestamp;
  } catch {
    return { ok: false, status: 400, reason: "The body is not JSON." };
  }
  if (typeof webhookTimestamp !== "number" || Math.abs(Date.now() - webhookTimestamp) > LINEAR_MAX_SKEW_MS) {
    return { ok: false, status: 401, reason: "Stale or missing webhookTimestamp." };
  }
  return { ok: true };
}
