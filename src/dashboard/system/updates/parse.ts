// A tiny Markdown parser for release notes: headings, bullets (with wrapped
// continuation lines), rules, paragraphs, **bold** and `code`. Everything else
// is plain text. `Markdown.tsx` turns the result into React elements (no HTML
// strings), so nothing in the notes can inject markup.

export type HeadingLevel = 1 | 2 | 3 | 4;

export type Block =
  | { kind: "heading"; level: HeadingLevel; text: string }
  | { kind: "list"; items: string[] }
  | { kind: "paragraph"; text: string }
  | { kind: "rule" };

export type Inline = { kind: "text" | "bold" | "code"; text: string };

/** Split Markdown into blocks. HTML comments are dropped; any other HTML stays as text. */
export function parseBlocks(markdown: string): Block[] {
  const lines = markdown.replace(/<!--[\s\S]*?-->/g, "").split(/\r?\n/);
  const blocks: Block[] = [];
  let list: string[] | null = null;
  let para: string[] | null = null;

  const flush = () => {
    if (list) blocks.push({ kind: "list", items: list });
    if (para) blocks.push({ kind: "paragraph", text: para.join(" ") });
    list = null;
    para = null;
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    if (!line.trim()) {
      flush();
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: "heading", level: heading[1].length as HeadingLevel, text: heading[2].trim() });
      continue;
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush();
      blocks.push({ kind: "rule" });
      continue;
    }
    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    if (bullet) {
      if (para) flush();
      list ??= [];
      list.push(bullet[1].trim());
      continue;
    }
    // An indented line under a bullet continues it.
    if (list && /^\s+/.test(line)) {
      list[list.length - 1] += ` ${line.trim()}`;
      continue;
    }
    if (list) flush();
    para ??= [];
    para.push(line.trim());
  }
  flush();
  return blocks;
}

/** Split one line into plain, **bold** and `code` runs. */
export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  const re = /\*\*(.+?)\*\*|`([^`]+)`/g;
  let at = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > at) out.push({ kind: "text", text: text.slice(at, m.index) });
    out.push(m[1] !== undefined ? { kind: "bold", text: m[1] } : { kind: "code", text: m[2] });
    at = m.index + m[0].length;
  }
  if (at < text.length) out.push({ kind: "text", text: text.slice(at) });
  return out;
}
