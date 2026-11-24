/**
 * Structural sharing for snapshots: returns `next`, except that every subtree
 * deep-equal to the same place in `prev` is `prev`'s own object. A heartbeat
 * that changes one field then keeps every other object's reference, so
 * selectors and memos that compare by reference see no change. When nothing
 * changed at all, the result is `prev` itself.
 *
 * Only plain objects and arrays are walked (a snapshot is JSON); anything else
 * is compared with `Object.is`.
 */
export function shareUnchanged<T>(prev: unknown, next: T): T {
  if (Object.is(prev, next)) return next;
  if (Array.isArray(next)) {
    if (!Array.isArray(prev)) return next;
    let same = prev.length === next.length;
    const out = next.map((item, i) => {
      const shared = shareUnchanged(prev[i], item);
      if (shared !== prev[i]) same = false;
      return shared;
    });
    return (same ? prev : out) as T;
  }
  if (!isPlainObject(next) || !isPlainObject(prev)) return next;
  const keys = Object.keys(next);
  let same = keys.length === Object.keys(prev).length;
  const out: Record<string, unknown> = {};
  for (const key of keys) {
    const had = Object.prototype.hasOwnProperty.call(prev, key);
    const shared = had ? shareUnchanged(prev[key], next[key]) : next[key];
    if (!had || shared !== prev[key]) same = false;
    out[key] = shared;
  }
  return (same ? prev : out) as T;
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  if (typeof v !== "object" || v === null) return false;
  const proto = Object.getPrototypeOf(v);
  return proto === Object.prototype || proto === null;
}
