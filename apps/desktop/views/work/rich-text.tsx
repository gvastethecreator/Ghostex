import { Fragment, useState, type ReactNode } from "react";
import type { WorkMedia } from "./types";

const URL_PATTERN = /(https?:\/\/[^\s<>"')\]]+)/gu;

/** Plain text with its links made clickable; Markdown marks stay as typed. */
export function LinkifiedText({
  text,
  onOpenUrl,
}: {
  text: string;
  onOpenUrl: (url: string) => void;
}) {
  const parts = text.split(URL_PATTERN);
  return (
    <>
      {parts.map((part, index) =>
        index % 2 === 1 ? (
          <a
            key={index}
            href={part}
            onClick={(event) => {
              event.preventDefault();
              onOpenUrl(part);
            }}
          >
            {part}
          </a>
        ) : (
          <Fragment key={index}>{part}</Fragment>
        ),
      )}
    </>
  );
}

/** The start of a long text, with Show more. */
export function ClampedText({
  text,
  limit = 700,
  onOpenUrl,
}: {
  text: string;
  limit?: number;
  onOpenUrl: (url: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const trimmed = text.trim();
  if (!trimmed) return <p className="w-faint">No description.</p>;
  const long = trimmed.length > limit;
  const shown =
    long && !expanded ? `${trimmed.slice(0, limit).trimEnd()}…` : trimmed;
  return (
    <div className="w-text">
      <LinkifiedText text={shown} onOpenUrl={onOpenUrl} />
      {long ? (
        <button
          type="button"
          className="w-link-btn"
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      ) : null}
    </div>
  );
}

/**
 * CDXC:WorkMode 2026-10-09 DECISION:
 * User: videos and Loom or YouTube links play right on the ticket's page, which is why the Work
 * page is a web page.
 */
export function MediaPlayer({ media }: { media: WorkMedia }): ReactNode {
  if (media.kind === "video") {
    return (
      <video
        className="w-video"
        src={media.embedUrl}
        controls
        preload="metadata"
      />
    );
  }
  return (
    <iframe
      className="w-video"
      src={media.embedUrl}
      title={media.kind === "loom" ? "Loom video" : "YouTube video"}
      allow="autoplay; fullscreen; picture-in-picture; encrypted-media"
      allowFullScreen
      referrerPolicy="strict-origin-when-cross-origin"
    />
  );
}
