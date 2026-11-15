import { describe, expect, it } from "vitest";
import { parseBlocks, parseInline } from "./parse";

describe("release notes parser", () => {
  it("reads headings, bullets with wrapped lines, rules and paragraphs", () => {
    const md = [
      "## v1.3.0",
      "",
      "### New",
      "",
      "- **Walk-away safety**: locking the PC",
      "  pauses the belt.",
      "* Second",
      "",
      "Plain text",
      "over two lines.",
      "---",
      "<!-- installer-note -->",
    ].join("\r\n");
    expect(parseBlocks(md)).toEqual([
      { kind: "heading", level: 2, text: "v1.3.0" },
      { kind: "heading", level: 3, text: "New" },
      { kind: "list", items: ["**Walk-away safety**: locking the PC pauses the belt.", "Second"] },
      { kind: "paragraph", text: "Plain text over two lines." },
      { kind: "rule" },
    ]);
  });

  it("keeps HTML as text and drops comments", () => {
    expect(parseBlocks("<img src=x onerror=alert(1)>\n<!-- hidden -->")).toEqual([
      { kind: "paragraph", text: "<img src=x onerror=alert(1)>" },
    ]);
    expect(parseBlocks("")).toEqual([]);
    expect(parseBlocks("#NoSpace")).toEqual([{ kind: "paragraph", text: "#NoSpace" }]);
  });

  it("splits bold and code runs", () => {
    expect(parseInline("A **bold** and `code` end")).toEqual([
      { kind: "text", text: "A " },
      { kind: "bold", text: "bold" },
      { kind: "text", text: " and " },
      { kind: "code", text: "code" },
      { kind: "text", text: " end" },
    ]);
    expect(parseInline("no ** pair")).toEqual([{ kind: "text", text: "no ** pair" }]);
    expect(parseInline("")).toEqual([]);
  });
});
