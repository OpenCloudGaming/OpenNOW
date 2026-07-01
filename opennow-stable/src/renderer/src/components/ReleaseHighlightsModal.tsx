import { createPortal } from "react-dom";
import { ExternalLink, X } from "lucide-react";
import type { JSX } from "react";
import type { ReleaseHighlightsPayload } from "@shared/gfn";
import { useTranslation } from "../i18n";

// ---------------------------------------------------------------------------
// Simple inline markdown renderer
// Handles the typical GitHub release note format without a library dependency.
// Supports: ATX headings (#, ##, ###), unordered lists (- / *),
//           **bold**, `code`, and [text](url) links.
// ---------------------------------------------------------------------------

function renderInline(text: string): JSX.Element[] {
  const parts: JSX.Element[] = [];
  let remaining = text;
  let key = 0;

  while (remaining.length > 0) {
    // Bold **text**
    const boldMatch = remaining.match(/^(.*?)\*\*(.+?)\*\*(.*)/s);
    // Inline code `code`
    const codeMatch = remaining.match(/^(.*?)`(.+?)`(.*)/s);
    // Link [text](url)
    const linkMatch = remaining.match(/^(.*?)\[([^\]]+)\]\((https?:\/\/[^)]+)\)(.*)/s);

    const firstBold = boldMatch ? boldMatch[1].length : Infinity;
    const firstCode = codeMatch ? codeMatch[1].length : Infinity;
    const firstLink = linkMatch ? linkMatch[1].length : Infinity;

    const firstIdx = Math.min(firstBold, firstCode, firstLink);

    if (firstIdx === Infinity) {
      // No more markup
      parts.push(<span key={key++}>{remaining}</span>);
      break;
    }

    if (firstIdx === firstBold && boldMatch) {
      if (boldMatch[1]) parts.push(<span key={key++}>{boldMatch[1]}</span>);
      parts.push(<strong key={key++}>{boldMatch[2]}</strong>);
      remaining = boldMatch[3];
    } else if (firstIdx === firstCode && codeMatch) {
      if (codeMatch[1]) parts.push(<span key={key++}>{codeMatch[1]}</span>);
      parts.push(<code key={key++} className="rh-inline-code">{codeMatch[2]}</code>);
      remaining = codeMatch[3];
    } else if (firstIdx === firstLink && linkMatch) {
      if (linkMatch[1]) parts.push(<span key={key++}>{linkMatch[1]}</span>);
      parts.push(
        <a
          key={key++}
          href="#"
          className="rh-link"
          onClick={(e) => {
            e.preventDefault();
            void window.openNow.openExternalUrl(linkMatch[3]);
          }}
        >
          {linkMatch[2]}
        </a>,
      );
      remaining = linkMatch[4];
    } else {
      parts.push(<span key={key++}>{remaining}</span>);
      break;
    }
  }

  return parts;
}

function renderMarkdown(markdown: string): JSX.Element[] {
  const lines = markdown.split(/\r?\n/);
  const elements: JSX.Element[] = [];
  let listItems: JSX.Element[] = [];
  let key = 0;

  const flushList = () => {
    if (listItems.length > 0) {
      elements.push(<ul key={key++} className="rh-list">{listItems}</ul>);
      listItems = [];
    }
  };

  for (const rawLine of lines) {
    const line = rawLine.trimEnd();

    // Headings
    const headingMatch = line.match(/^(#{1,3})\s+(.*)/);
    if (headingMatch) {
      flushList();
      const level = headingMatch[1].length;
      const content = renderInline(headingMatch[2].trim());
      if (level === 1) {
        elements.push(<h2 key={key++} className="rh-h2">{content}</h2>);
      } else if (level === 2) {
        elements.push(<h3 key={key++} className="rh-h3">{content}</h3>);
      } else {
        elements.push(<h4 key={key++} className="rh-h4">{content}</h4>);
      }
      continue;
    }

    // List items (- or * or numbered)
    const listMatch = line.match(/^[\s]*[-*+]\s+(.*)/);
    if (listMatch) {
      listItems.push(
        <li key={key++} className="rh-list-item">
          {renderInline(listMatch[1].trim())}
        </li>,
      );
      continue;
    }

    // Blank line
    if (line.trim() === "") {
      flushList();
      continue;
    }

    // Paragraph
    flushList();
    elements.push(
      <p key={key++} className="rh-paragraph">
        {renderInline(line.trim())}
      </p>,
    );
  }

  flushList();
  return elements;
}

// ---------------------------------------------------------------------------
// Modal component
// ---------------------------------------------------------------------------

export interface ReleaseHighlightsModalProps {
  payload: ReleaseHighlightsPayload;
  /**
   * Called when the user dismisses the modal via "Got it".
   * The caller decides whether to ack (auto-show) or just close (manual-show).
   */
  onDismiss: () => void;
  /** Called for manual "View on GitHub" link */
  version: string;
}

export function ReleaseHighlightsModal({
  payload,
  onDismiss,
  version,
}: ReleaseHighlightsModalProps): JSX.Element | null {
  const { t } = useTranslation();

  const githubReleaseUrl = `https://github.com/OpenCloudGaming/OpenNOW/releases/tag/v${version}`;

  if (typeof document === "undefined") return null;

  return createPortal(
    <div
      className="rh-overlay"
      role="dialog"
      aria-modal="true"
      aria-label={t("releaseHighlights.kicker")}
    >
      <button
        type="button"
        className="rh-backdrop"
        onClick={onDismiss}
        aria-label={t("app.actions.close")}
      />

      <div className="rh-card">
        {/* Header */}
        <div className="rh-header">
          <div className="rh-kicker">{t("releaseHighlights.kicker")}</div>
          <h2 className="rh-title">{payload.title}</h2>
          <button
            type="button"
            className="rh-close-btn"
            onClick={onDismiss}
            aria-label={t("app.actions.close")}
          >
            <X size={18} />
          </button>
        </div>

        {/* Scrollable body */}
        <div className="rh-body">
          {payload.source === "fallback" ? (
            <p className="rh-fallback-text">{t("releaseHighlights.unavailable")}</p>
          ) : (
            <div className="rh-markdown">{renderMarkdown(payload.bodyMarkdown)}</div>
          )}
        </div>

        {/* Footer actions */}
        <div className="rh-footer">
          <button
            type="button"
            className="rh-btn-secondary"
            onClick={() => {
              void window.openNow.openExternalUrl(githubReleaseUrl);
            }}
          >
            <ExternalLink size={14} />
            {t("releaseHighlights.viewOnGitHub")}
          </button>
          <button
            type="button"
            className="rh-btn-primary"
            onClick={onDismiss}
            autoFocus
          >
            {t("releaseHighlights.gotIt")}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
