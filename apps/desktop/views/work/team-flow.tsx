import { IconCheck, IconQuestionMark, IconX } from "@tabler/icons-react";
import { cx } from "./components";
import type { TeamFlowStepState } from "./types";

/**
 * The ticket's place in the team's flow. A step whose data Ghostex does not have yet (Slack,
 * the video OK) shows as unknown, never as done (server/src/work_mode/team_flow.rs).
 */
export function TeamFlowTracker({ steps }: { steps: TeamFlowStepState[] }) {
  if (steps.length === 0) return null;
  return (
    <ol className="w-flow team-flow-tracker" aria-label="Team flow">
      {steps.map((step, index) => (
        <li
          key={step.id}
          className={cx("w-flow-step", `is-${step.status}`, `step-${step.id}`)}
          title={`${step.label}: ${stepStatusText(step)}`}
        >
          <span className="w-flow-node">
            {step.status === "done" ? (
              <IconCheck size={11} stroke={3} />
            ) : step.status === "failed" ? (
              <IconX size={11} stroke={3} />
            ) : step.status === "unknown" ? (
              <IconQuestionMark size={10} stroke={2.5} />
            ) : (
              <span className="w-flow-index">{index + 1}</span>
            )}
          </span>
          <span className="w-flow-label">{step.label}</span>
          <span className="w-flow-sub">{step.detail}</span>
        </li>
      ))}
    </ol>
  );
}

function stepStatusText(step: TeamFlowStepState): string {
  switch (step.status) {
    case "done":
      return `done${step.detail ? ` (${step.detail})` : ""}`;
    case "current":
      return `next${step.detail ? ` (${step.detail})` : ""}`;
    case "failed":
      return `needs attention${step.detail ? ` (${step.detail})` : ""}`;
    case "unknown":
      return "unknown: Ghostex cannot see this step yet";
    default:
      return step.detail || "not yet";
  }
}
