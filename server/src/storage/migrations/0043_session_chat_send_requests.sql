-- CDXC:SessionChat 2026-10-05 WHY:
-- The send ledger behind `sendRequestId` (session_chat_send_requests.rs): one row per chat send a
-- client or gxserver itself made, so a retried send returns the first attempt's answer instead of
-- typing the message again. `state` is `sending` while the attempt runs, `sent` with the RPC result
-- kept in `response`, or `refused` (nothing was submitted, so the same id may try again). A row
-- still `sending` after a restart is ambiguous and is settled against the agent's transcript, which
-- `needles` (the coordinators' delivery match for the text) and `createdAtMs` make possible. Rows
-- are pruned after 24 hours and to the newest 500 per session.
CREATE TABLE session_chat_send_requests (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  sendRequestId TEXT NOT NULL,
  endpoint TEXT NOT NULL,
  state TEXT NOT NULL,
  needles TEXT NOT NULL DEFAULT '[]',
  response TEXT,
  createdAtMs INTEGER NOT NULL,
  updatedAtMs INTEGER NOT NULL,
  PRIMARY KEY (projectId, sessionId, sendRequestId)
);

PRAGMA user_version = 43;
