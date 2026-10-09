/**
 * The Linear calls the Slack command flow makes with the team's Linear key (`LINEAR_API_KEY`): the team keys that tell a ticket ID from "UTF-8", a ticket's details, and creating a ticket.
 *
 * CDXC:TeamSync 2026-10-09 WHY:
 * The ticket is found or created in Convex, not on the requester's computer, because the flow must work while that computer is off: the requester is told "I've created SPX-1253 and its working thread" and the session starts when their Ghostex is back. `LINEAR_API_URL` points the calls at a mock for tests.
 */

export type LinearTeam = { id: string; key: string; name: string };

export type LinearIssue = {
  identifier: string;
  title: string;
  url: string;
  branchName: string | null;
  description: string | null;
  teamKey: string | null;
  teamName: string | null;
  assignee: string | null;
  state: string | null;
  labels: string[];
  comments: { author: string | null; body: string; createdAt: string }[];
  attachments: { title: string | null; url: string }[];
};

export function hasLinearKey(): boolean {
  return Boolean(process.env.LINEAR_API_KEY);
}

async function linearGraphql(query: string, variables: Record<string, unknown>): Promise<any> {
  const key = process.env.LINEAR_API_KEY;
  if (!key) throw new Error("LINEAR_API_KEY is not set on this deployment. Run `ghostex team linear-connect`.");
  const response = await fetch(process.env.LINEAR_API_URL ?? "https://api.linear.app/graphql", {
    method: "POST",
    headers: { authorization: key, "content-type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  const body = (await response.json().catch(() => null)) as { data?: any; errors?: { message?: string }[] } | null;
  if (!response.ok || !body || body.errors?.length) {
    throw new Error(`Linear: ${body?.errors?.[0]?.message ?? `HTTP ${response.status}`}`);
  }
  return body.data;
}

export async function linearTeams(): Promise<LinearTeam[]> {
  const data = await linearGraphql("query { teams(first: 250) { nodes { id key name } } }", {});
  return (data?.teams?.nodes ?? []) as LinearTeam[];
}

const ISSUE_FIELDS = `identifier title url branchName description
  team { key name } assignee { name } state { name }
  labels(first: 20) { nodes { name } }
  comments(first: 50) { nodes { body createdAt user { name } } }
  attachments(first: 20) { nodes { title url } }`;

function parseIssue(issue: any): LinearIssue | null {
  if (!issue?.identifier) return null;
  return {
    identifier: issue.identifier,
    title: issue.title ?? issue.identifier,
    url: issue.url ?? "",
    branchName: issue.branchName ?? null,
    description: issue.description ?? null,
    teamKey: issue.team?.key ?? null,
    teamName: issue.team?.name ?? null,
    assignee: issue.assignee?.name ?? null,
    state: issue.state?.name ?? null,
    labels: (issue.labels?.nodes ?? []).map((label: any) => String(label.name)),
    comments: (issue.comments?.nodes ?? []).map((comment: any) => ({
      author: comment.user?.name ?? null,
      body: String(comment.body ?? ""),
      createdAt: String(comment.createdAt ?? ""),
    })),
    attachments: (issue.attachments?.nodes ?? []).map((attachment: any) => ({
      title: attachment.title ?? null,
      url: String(attachment.url ?? ""),
    })),
  };
}

export async function linearIssue(identifier: string): Promise<LinearIssue | null> {
  const data = await linearGraphql(`query($id: String!) { issue(id: $id) { ${ISSUE_FIELDS} } }`, { id: identifier }).catch(
    (error: Error) => {
      // Linear answers an unknown identifier with an "Entity not found" error.
      if (/not found/i.test(error.message)) return null;
      throw error;
    },
  );
  return parseIssue(data?.issue);
}

export async function createLinearIssue(input: {
  teamId: string;
  title: string;
  description: string;
  assigneeId?: string;
}): Promise<LinearIssue> {
  const data = await linearGraphql(
    `mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { ${ISSUE_FIELDS} } } }`,
    { input },
  );
  const issue = data?.issueCreate?.success ? parseIssue(data.issueCreate.issue) : null;
  if (!issue) throw new Error("Linear did not create the ticket.");
  return issue;
}
