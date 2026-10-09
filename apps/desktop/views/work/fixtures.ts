/**
 * Sample answers for the page's dev mode (`work.html?fixtures=1`), shaped exactly like gxserver's
 * and the desktop bridge's, so the page can be drawn and screenshotted without the app. Loaded
 * only in that mode.
 */
import type { WorkItem, WorkItemDetails, WorkList, WorkReady } from "./types";

const minutesAgo = (minutes: number) =>
  new Date(Date.now() - minutes * 60_000).toISOString();

const session = (title: string, working = false) => ({
  projectId: "p-shortpoint",
  sessionId: `s-${title}`,
  title,
  working,
  lifecycle: "running",
  agentId: "claude",
});

const base: Pick<
  WorkItem,
  "labels" | "noTicket" | "sessions" | "assignedToMe"
> = {
  labels: [],
  noTicket: false,
  sessions: [],
  assignedToMe: false,
};

const ITEMS: WorkItem[] = [
  {
    ...base,
    key: "linear:SPX-1250",
    kind: "linearIssue",
    id: "SPX-1250",
    title: "EasyPass share dialog ignores dark theme",
    updatedAt: minutesAgo(0),
    status: { group: "progress", name: "In Progress" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: { name: "EasyPass" },
    assignee: { name: "Sami", isMe: false },
    pullRequest: { number: 6555, state: "draft", checks: "pending" },
    linearIssue: "SPX-1250",
    slackThreadCount: 1,
  },
  {
    ...base,
    key: "linear:SPX-1241",
    kind: "linearIssue",
    id: "SPX-1241",
    title: "Table element loses column widths after paste",
    updatedAt: minutesAgo(1),
    status: { group: "progress", name: "In Progress" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: { name: "Table element" },
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: { number: 6551, state: "draft", checks: "pending" },
    sessions: [session("table-paste-widths", true)],
    linearIssue: "SPX-1241",
    slackThreadCount: 3,
  },
  {
    ...base,
    key: "linear:SPX-1234",
    kind: "linearIssue",
    id: "SPX-1234",
    title: "EasyPass Live mode disappears when a table is on the page",
    updatedAt: minutesAgo(12),
    status: { group: "review", name: "In Review" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: { name: "EasyPass" },
    cycle: "Sprint 20",
    labels: ["Bug"],
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: {
      number: 6538,
      state: "open",
      checks: "passing",
      url: "https://github.com/shortpoint/shortpoint/pull/6538",
    },
    sessions: [session("live-mode-table")],
    branchName: "yahia/spx-1234-live-mode-table",
    linearIssue: "SPX-1234",
    slackThreadCount: 2,
  },
  {
    ...base,
    key: "linear:SPX-1238",
    kind: "linearIssue",
    id: "SPX-1238",
    title: "Sign-up form accepts emails without a domain ending",
    updatedAt: minutesAgo(25),
    status: { group: "review", name: "In Review" },
    projectId: "p-website",
    projectName: "shortpoint-website",
    linearProject: { name: "Website sign-up" },
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    pullRequest: { number: 212, state: "open", checks: "failing" },
    sessions: [{ ...session("signup-email-domain"), projectId: "p-website" }],
    linearIssue: "SPX-1238",
    slackThreadCount: 1,
  },
  {
    ...base,
    key: "pr:shortpoint/shortpoint#6552",
    kind: "pullRequest",
    id: "#6552",
    title: "Speed up table render tests",
    updatedAt: minutesAgo(120),
    status: { group: "open", name: "Open" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    pullRequest: { number: 6552, state: "open", checks: "passing" },
    noTicket: true,
    pullRequestRef: "https://github.com/shortpoint/shortpoint/pull/6552",
  },
  {
    ...base,
    key: "linear:SPX-1239",
    kind: "linearIssue",
    id: "SPX-1239",
    title: "EasyPass token refresh fails after 24 hours",
    updatedAt: minutesAgo(180),
    status: { group: "review", name: "QA" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: { name: "EasyPass" },
    assignee: { name: "Lina", isMe: false },
    pullRequest: { number: 6544, state: "open", checks: "passing" },
    linearIssue: "SPX-1239",
    slackThreadCount: 2,
  },
  {
    ...base,
    key: "issue:shortpoint/shortpoint-website#218",
    kind: "githubIssue",
    id: "#218",
    title: "Sign-up page: Arabic text overflows the plan cards",
    updatedAt: minutesAgo(240),
    status: { group: "todo", name: "Open" },
    projectId: "p-website",
    projectName: "shortpoint-website",
    assignee: { name: "yahia", isMe: true },
    assignedToMe: true,
    githubIssue: 218,
  },
  {
    ...base,
    key: "linear:SPX-1245",
    kind: "linearIssue",
    id: "SPX-1245",
    title: "Add “Copy link” to the EasyPass share menu",
    updatedAt: minutesAgo(60 * 26),
    status: { group: "todo", name: "Todo" },
    projectId: "p-shortpoint",
    projectName: "shortpoint",
    linearProject: { name: "EasyPass" },
    cycle: "Sprint 20",
    labels: ["Feature"],
    assignee: { name: "Yahia", isMe: true },
    assignedToMe: true,
    branchName: "yahia/spx-1245-copy-link",
    linearIssue: "SPX-1245",
    slackThreadCount: 1,
  },
];

const LIST: WorkList = {
  items: ITEMS,
  projects: [
    {
      projectId: "p-shortpoint",
      name: "shortpoint",
      repo: "shortpoint/shortpoint",
    },
    {
      projectId: "p-website",
      name: "shortpoint-website",
      repo: "shortpoint/shortpoint-website",
    },
  ],
  viewer: { githubLogin: "yahia" },
  linearConfigured: true,
  ghAvailable: true,
  errors: [],
  generatedAt: new Date().toISOString(),
  refreshing: false,
};

const READY: WorkReady = {
  projectIds: ["p-shortpoint", "p-website"],
  agents: [
    { id: "claude", name: "Claude", primary: true },
    { id: "codex", name: "Codex", primary: false },
  ],
  pendingOpen: null,
};

function details(item: WorkItem): WorkItemDetails {
  const isLinear = item.kind === "linearIssue";
  return {
    item,
    linear: isLinear
      ? {
          identifier: item.id,
          title: item.title,
          url: `https://linear.app/shortpoint/issue/${item.id.toLowerCase()}`,
          description:
            item.id === "SPX-1234"
              ? "Live mode disappears as soon as a Table element is on the page. Happens in the sandbox and on 9.199.\n\nSteps: add a Table, switch to Live mode, refresh.\n\nRepro: https://www.loom.com/share/5bbdeb480ba84e65b1b3de8c190e2003"
              : "People want to copy a share link without opening the share dialog. Add “Copy link” to the share menu, and show a short “Link copied” toast.",
          stateName: item.status.name,
          stateType: item.status.group === "todo" ? "unstarted" : "started",
          cycle: item.cycle,
          labels: item.labels,
          comments: [
            {
              author: "Rana",
              body: "Confirmed on 9.199.0.402, in Edge and Chrome.",
              createdAt: minutesAgo(300),
            },
            {
              author: "Omar",
              body: "It comes back if you switch tabs.",
              createdAt: minutesAgo(200),
            },
          ],
          commentCount: 2,
          attachments: [],
        }
      : null,
    githubIssue:
      item.kind === "githubIssue"
        ? {
            number: item.githubIssue ?? 0,
            title: item.title,
            url: "https://github.com/shortpoint/shortpoint-website/issues/218",
            state: "open",
            body: "On the Arabic sign-up page the plan names overflow their cards at 1280px.",
            comments: [],
            commentCount: 0,
          }
        : null,
    pullRequest: item.pullRequest
      ? {
          number: item.pullRequest.number,
          title:
            item.id === "SPX-1234"
              ? "Fix Live mode unmounting with a Table element"
              : (item.pullRequest.title ?? item.title),
          url:
            item.pullRequest.url ??
            `https://github.com/shortpoint/shortpoint/pull/${item.pullRequest.number}`,
          state: item.pullRequest.state,
          headBranch: item.branchName ?? null,
          reviewDecision:
            item.pullRequest.checks === "passing" ? "APPROVED" : null,
          reviews: {
            approved: item.pullRequest.checks === "passing" ? 1 : 0,
            changesRequested: 0,
            commented: 1,
          },
          unresolvedReviewThreads:
            item.pullRequest.checks === "failing" ? 2 : 0,
          labels: [],
          checks: [
            {
              name: "build-spfx",
              workflow: "CI",
              status: "passed",
              durationSeconds: 372,
            },
            {
              name: "unit-tests",
              workflow: "CI",
              status:
                item.pullRequest.checks === "failing" ? "failed" : "passed",
              durationSeconds: 220,
            },
            {
              name: "e2e-live-mode",
              workflow: "E2E",
              status:
                item.pullRequest.checks === "pending" ? "pending" : "passed",
              durationSeconds: 483,
            },
            {
              name: "Greptile review",
              status: "passed",
              durationSeconds: null,
            },
          ],
          checksSummary:
            item.pullRequest.checks === "failing"
              ? { total: 4, passed: 3, failed: 1, pending: 0, skipped: 0 }
              : item.pullRequest.checks === "pending"
                ? { total: 4, passed: 3, failed: 0, pending: 1, skipped: 0 }
                : { total: 4, passed: 4, failed: 0, pending: 0, skipped: 0 },
        }
      : null,
    media:
      item.id === "SPX-1234"
        ? [
            {
              kind: "loom",
              url: "https://www.loom.com/share/5bbdeb480ba84e65b1b3de8c190e2003",
              embedUrl:
                "https://www.loom.com/embed/5bbdeb480ba84e65b1b3de8c190e2003",
            },
          ]
        : [],
    links:
      item.id === "SPX-1234"
        ? [
            {
              title: "root-cause.html",
              subtitle: "Uploaded by Yahia",
              url: "https://example.com/root-cause.html",
            },
          ]
        : [],
    teamFlow: {
      source: "builtIn",
      steps:
        item.id === "SPX-1234"
          ? [
              {
                id: "ticket",
                label: "Ticket",
                status: "done",
                detail: "SPX-1234",
              },
              {
                id: "working-thread",
                label: "Working thread",
                status: "unknown",
                detail: "not connected",
              },
              {
                id: "session",
                label: "Session",
                status: "done",
                detail: "1 session",
              },
              { id: "pr", label: "PR", status: "done", detail: "#6538" },
              {
                id: "review-comments",
                label: "Review comments",
                status: "done",
                detail: "0 open",
              },
              { id: "ci", label: "CI", status: "done", detail: "4 / 4" },
              {
                id: "video",
                label: "Video",
                status: "unknown",
                detail: "not connected",
              },
              {
                id: "qc-package",
                label: "QC package",
                status: "current",
                detail: "no READY-FOR-QC",
              },
              {
                id: "validation",
                label: "Validation",
                status: "unknown",
                detail: "not connected",
              },
            ]
          : [
              {
                id: "ticket",
                label: "Ticket",
                status: "done",
                detail: item.id,
              },
              {
                id: "working-thread",
                label: "Working thread",
                status: "unknown",
                detail: "not connected",
              },
              {
                id: "session",
                label: "Session",
                status: item.sessions.length ? "done" : "current",
                detail: item.sessions.length ? "1 session" : "none yet",
              },
              {
                id: "pr",
                label: "PR",
                status: "pending",
                detail: "not opened",
              },
              {
                id: "review-comments",
                label: "Review comments",
                status: "pending",
                detail: "after PR",
              },
              { id: "ci", label: "CI", status: "pending", detail: "after PR" },
              {
                id: "video",
                label: "Video",
                status: "unknown",
                detail: "not connected",
              },
              {
                id: "qc-package",
                label: "QC package",
                status: "pending",
                detail: "after PR",
              },
              {
                id: "validation",
                label: "Validation",
                status: "unknown",
                detail: "not connected",
              },
            ],
    },
    projects: LIST.projects,
    errors: [],
  };
}

export async function answerFromFixtures(
  action: string,
  params: Record<string, unknown>,
): Promise<unknown> {
  await new Promise((resolve) => setTimeout(resolve, 60));
  switch (action) {
    case "work.ready":
      return READY;
    case "work.list":
      return { ...LIST, generatedAt: new Date().toISOString() };
    case "work.read": {
      const item =
        ITEMS.find(
          (candidate) =>
            (params.linearIssue &&
              candidate.linearIssue === params.linearIssue) ||
            (params.githubIssue &&
              candidate.githubIssue === params.githubIssue) ||
            (params.pullRequest &&
              candidate.pullRequestRef === params.pullRequest),
        ) ?? ITEMS[0];
      return details(item as WorkItem);
    }
    case "work.startChat":
      return {
        projectId: params.projectId,
        sessionId: "s-new",
        branch: "yahia/spx-1245-copy-link",
      };
    default:
      return { ok: true };
  }
}
