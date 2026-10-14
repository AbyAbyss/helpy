// Answers as rich text: GitHub-style Markdown (tables, task lists,
// strikethrough), math, highlighted code, web images, and Mermaid diagrams
// and charts. Mermaid is large, so it loads only when a reply has a diagram.

import { useEffect, useId, useState, type ReactNode } from "react";
import Markdown, { type Components } from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "katex/dist/katex.min.css";
import { api } from "./ipc";
import { linkify, WebLink } from "./links";
import "./markdown.css";

type Hast = { type: string; tagName?: string; value?: string; properties?: { className?: unknown }; children?: Hast[] };

const textOf = (n: Hast): string => (n.type === "text" ? (n.value ?? "") : (n.children ?? []).map(textOf).join(""));

/** The Mermaid source when a code block is ```mermaid. */
function mermaidSource(pre?: Hast): string | null {
  const code = pre?.children?.find((c) => c.tagName === "code");
  const cls = code?.properties?.className;
  return Array.isArray(cls) && cls.includes("language-mermaid") ? textOf(code!) : null;
}

const dark = () => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;

/** Helpy's colours for charts, strong enough to read on light and dark. */
const PALETTE = ["#e5484d", "#3b82f6", "#22a06b", "#f59e0b", "#8b5cf6", "#06b6d4", "#ec4899", "#84cc16"];

function themeVariables(d: boolean) {
  const pies = Object.fromEntries(PALETTE.map((c, i) => [`pie${i + 1}`, c]));
  return {
    ...pies,
    darkMode: d,
    background: "transparent",
    primaryColor: d ? "#2a303c" : "#fdeced",
    primaryBorderColor: "#e5484d",
    primaryTextColor: d ? "#e8eaf0" : "#171b26",
    lineColor: d ? "#8b93a7" : "#7b8395",
    textColor: d ? "#e8eaf0" : "#171b26",
    pieStrokeColor: d ? "#1a1e27" : "#ffffff",
    pieOuterStrokeColor: "transparent",
    pieSectionTextColor: "#ffffff",
    xyChart: {
      plotColorPalette: PALETTE.join(", "),
      backgroundColor: "transparent",
      ...Object.fromEntries(
        ["titleColor", "xAxisLabelColor", "xAxisTitleColor", "xAxisTickColor", "xAxisLineColor", "yAxisLabelColor", "yAxisTitleColor", "yAxisTickColor", "yAxisLineColor"].map((k) => [k, d ? "#c9cdd8" : "#4a5162"]),
      ),
    },
  };
}

/**
 * A Mermaid diagram or chart. While a reply is still streaming the source is
 * often incomplete, so it's drawn a moment after it stops changing, and shown
 * as code if it doesn't parse. `light` draws it for a white card whatever the
 * system theme.
 */
export function Diagram({ source, light = false }: { source: string; light?: boolean }) {
  const id = `mmd-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  const [svg, setSvg] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    const t = setTimeout(async () => {
      try {
        const { default: mermaid } = await import("mermaid");
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: "base", themeVariables: themeVariables(!light && dark()), fontFamily: "inherit" });
        const { svg } = await mermaid.render(id, source.trim());
        if (live) {
          setSvg(svg);
          setFailed(false);
        }
      } catch {
        // Mermaid leaves its error drawing behind; it isn't wanted.
        document.getElementById(`d${id}`)?.remove();
        if (live) setFailed(true);
      }
    }, 250);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [id, source, light]);
  if (svg && !failed) return <div className="md-diagram" dangerouslySetInnerHTML={{ __html: svg }} />;
  return (
    <pre className="md-diagram-src">
      <code>{source}</code>
    </pre>
  );
}

const SAFE_IMAGE = /^(https?:\/\/|data:image\/)/i;

/** A picture from the reply; it opens in the browser when links are on. */
function Picture({ src, alt, links }: { src?: string; alt?: string; links: boolean }) {
  const [broken, setBroken] = useState(false);
  if (!src || !SAFE_IMAGE.test(src) || broken) return alt ? <span className="md-img-alt">[{alt}]</span> : null;
  const img = <img className="md-img" src={src} alt={alt ?? ""} loading="lazy" referrerPolicy="no-referrer" onError={() => setBroken(true)} />;
  if (!links || !/^https?:/i.test(src)) return img;
  return (
    <a href={src} title={alt || src} onClick={(e) => (e.preventDefault(), api.openLink(src).catch(() => {}))}>
      {img}
    </a>
  );
}

/**
 * Math as models write it, in the form remark-math reads: "\( x \)" inline,
 * "\[ x \]" displayed, and "$$ x $$" alone on a line displayed too.
 */
const normalizeMath = (text: string) =>
  text
    .replace(/\\\[([\s\S]+?)\\\]/g, (_, m) => `\n$$\n${m.trim()}\n$$\n`)
    .replace(/\\\((.+?)\\\)/g, (_, m) => `$${m.trim()}$`)
    .replace(/^[ \t]*\$\$(.+?)\$\$[ \t]*$/gm, (_, m) => `$$\n${m.trim()}\n$$`);

/**
 * Markdown from a model. With `links` off (small cards, the circle overlay)
 * links stay plain text so a stray click doesn't open the browser.
 */
export function RichText({ text, links = true }: { text: string; links?: boolean }) {
  const components: Components = {
    a: links ? WebLink : ({ children }: { children?: ReactNode }) => <span>{children}</span>,
    img: ({ src, alt }) => <Picture src={typeof src === "string" ? src : undefined} alt={alt} links={links} />,
    table: ({ children }) => (
      <div className="md-table">
        <table>{children}</table>
      </div>
    ),
    pre: ({ node, children }) => {
      const source = mermaidSource(node as Hast | undefined);
      return source != null ? <Diagram source={source} /> : <pre className="md-pre">{children}</pre>;
    },
  };
  return (
    <Markdown
      remarkPlugins={[remarkGfm, [remarkMath, { singleDollarTextMath: true }]]}
      rehypePlugins={[rehypeKatex, [rehypeHighlight, { detect: false }]]}
      components={components}
    >
      {normalizeMath(links ? linkify(text) : text)}
    </Markdown>
  );
}
