import type { ReactNode } from "react";
import { api } from "./ipc";

const WEB = /^https?:\/\//i;

/**
 * Turns bare web addresses in Markdown into autolinks (`<https://…>`), so
 * they render as links too. Addresses already inside a link or autolink are
 * left alone, and trailing punctuation stays outside the link.
 */
export function linkify(markdown: string): string {
  return markdown.replace(/(?<!\]\(|<|[\w/])https?:\/\/[^\s<>()]*[^\s<>().,;:!?'"]/gi, (url) => `<${url}>`);
}

/**
 * A link in an answer. Web links open in the user's browser, since following
 * one would navigate Helpy's own window away; anything else stays text.
 */
export function WebLink({ href, children }: { href?: string; children?: ReactNode }) {
  if (!href || !WEB.test(href)) return <span className="md-link" title={href}>{children}</span>;
  return (
    <a
      className="md-link"
      href={href}
      title={`Open ${href}`}
      onClick={(e) => {
        e.preventDefault();
        api.openLink(href).catch(() => {});
      }}
    >
      {children}
    </a>
  );
}
