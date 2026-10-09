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

/** Slack's link forms (`<url|label>`, `<url>`) and bare links, then `*bold*`, `_italic_` and `` `code` ``. */
const SLACK_TOKEN =
  /<(https?:\/\/[^>|]+)\|([^>]+)>|<(https?:\/\/[^>]+)>|(https?:\/\/[^\s<>"')\]]+)|`([^`\n]+)`|\*([^*\n]+)\*|(?<![\w])_([^_\n]+)_(?![\w])|(?<![\w])(@[\w\-]+(?:\.[\w\-]+)*)/gu;

function unescapeSlack(text: string): string {
  return text
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&amp;/gu, "&");
}

/**
 * A Slack message as Slack draws it: links clickable, bold, italic, inline code and mentions.
 * gxserver's team data names mentions already (`@Rana`, `#bugs`), so only the marks are left.
 */
export function SlackText({
  text,
  onOpenUrl,
}: {
  text: string;
  onOpenUrl: (url: string) => void;
}) {
  const nodes: ReactNode[] = [];
  let last = 0;
  const link = (url: string, label: string, key: number) => (
    <a
      key={key}
      href={url}
      onClick={(event) => {
        event.preventDefault();
        onOpenUrl(url);
      }}
    >
      {unescapeSlack(label)}
    </a>
  );
  for (const match of text.matchAll(SLACK_TOKEN)) {
    const at = match.index ?? 0;
    if (at > last) nodes.push(unescapeSlack(text.slice(last, at)));
    const key = at;
    const [
      ,
      labelledUrl,
      label,
      angledUrl,
      bareUrl,
      code,
      bold,
      italic,
      mention,
    ] = match;
    if (labelledUrl) nodes.push(link(labelledUrl, label ?? labelledUrl, key));
    else if (angledUrl) nodes.push(link(angledUrl, angledUrl, key));
    else if (bareUrl) nodes.push(link(bareUrl, bareUrl, key));
    else if (code)
      nodes.push(
        <code key={key} className="w-code">
          {unescapeSlack(code)}
        </code>,
      );
    else if (bold) nodes.push(<strong key={key}>{unescapeSlack(bold)}</strong>);
    else if (italic) nodes.push(<em key={key}>{unescapeSlack(italic)}</em>);
    else if (mention)
      nodes.push(
        <span key={key} className="w-mention">
          {mention}
        </span>,
      );
    last = at + match[0].length;
  }
  if (last < text.length) nodes.push(unescapeSlack(text.slice(last)));
  return <>{nodes}</>;
}
