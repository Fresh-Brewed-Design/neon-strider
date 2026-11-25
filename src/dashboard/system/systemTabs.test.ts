import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  SYSTEM_TAB_KEY,
  SYSTEM_TABS,
  openSystemTab,
  readSystemTab,
  showSystemSection,
  subscribeSystemTab,
  tabOfSection,
} from "./systemTabs";

/** Every `id="sys-..."` a System section is given, read from the dashboard's source. */
function sectionIdsInSource(): string[] {
  const root = join(__dirname, "..");
  const ids = new Set<string>();
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (name.endsWith(".tsx") && !name.endsWith(".test.tsx"))
        for (const m of readFileSync(path, "utf8").matchAll(/id="(sys-[a-z-]+)"/g)) ids.add(m[1]);
    }
  };
  walk(root);
  return [...ids].sort();
}

describe("SYSTEM_TABS", () => {
  it("puts each System section on exactly one tab", () => {
    const ids = sectionIdsInSource();
    expect(ids.length).toBeGreaterThanOrEqual(15);
    for (const id of ids) {
      expect(SYSTEM_TABS.filter((t) => t.sections.includes(id)).map((t) => t.id), id).toHaveLength(1);
    }
    // And names no section that does not exist.
    expect(SYSTEM_TABS.flatMap((t) => t.sections).sort()).toEqual(ids);
  });

  it("groups the sections the way issue 359 asks", () => {
    expect(tabOfSection("sys-calibration")).toBe("deck");
    expect(tabOfSection("sys-strava")).toBe("links");
    expect(tabOfSection("sys-shortcuts")).toBe("routine");
    expect(tabOfSection("sys-updates")).toBe("core");
    expect(tabOfSection("sys-nothing")).toBeNull();
  });
});

describe("the open tab", () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.useRealTimers());

  it("is DECK until one is remembered, and ignores an unknown value", () => {
    expect(readSystemTab()).toBe("deck");
    localStorage.setItem(SYSTEM_TAB_KEY, "nope");
    expect(readSystemTab()).toBe("deck");
  });

  it("is remembered and told to the open page", () => {
    const seen: string[] = [];
    const stop = subscribeSystemTab((t) => seen.push(t));
    openSystemTab("core");
    stop();
    openSystemTab("links");
    expect(seen).toEqual(["core"]);
    expect(readSystemTab()).toBe("links");
  });

  it("follows a jump to a section, then scrolls to the section", () => {
    vi.useFakeTimers();
    const el = document.createElement("section");
    el.id = "sys-updates";
    el.scrollIntoView = vi.fn();
    document.body.append(el);
    showSystemSection("sys-updates");
    expect(readSystemTab()).toBe("core");
    expect(el.scrollIntoView).not.toHaveBeenCalled();
    vi.advanceTimersByTime(80);
    expect(el.scrollIntoView).toHaveBeenCalledWith({ block: "start" });
    el.remove();
  });
});
