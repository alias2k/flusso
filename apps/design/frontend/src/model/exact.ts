// JSON with integers past 2^53 kept exact. A JS number rounds them
// (9007199254740993 → …992), so a `BigInt` value loaded, typed or duplicated
// would be saved as another number. Where the browser has `JSON.rawJSON` and
// the reviver's source text, such an integer is held as raw JSON: it prints
// back as its own digits, so the model posts what the file holds.
//
// - `parseJson` reads every API reply.
// - `cloneJson` copies a piece of the model (`structuredClone` would turn a raw
//   JSON value into a plain object).
// - `exactIntegers` is false in a browser that can't do this; the inspector then
//   refuses such an integer instead of rounding it.

type RawJson = (text: string) => unknown;

const rawJson = (JSON as unknown as { rawJSON?: RawJson }).rawJSON;

/// Whether this browser keeps integers past 2^53 exact.
export const exactIntegers = typeof rawJson === "function";

/// `JSON.parse`, keeping an integer past 2^53 as its digits where it can.
export function parseJson(text: string): unknown {
  return JSON.parse(text, (_key: string, value: unknown, context?: { source?: string }) => {
    const source = context?.source;
    if (rawJson && typeof value === "number" && !Number.isSafeInteger(value) && source && /^-?\d+$/.test(source)) {
      return rawJson(source);
    }
    return value;
  });
}

/// A deep copy of JSON data that keeps exact integers exact.
export function cloneJson<T>(value: T): T {
  return parseJson(JSON.stringify(value)) as T;
}

/// Whether `value` holds an integer a JS number has already rounded.
export function hasRoundedInteger(value: unknown): boolean {
  if (typeof value === "number") return Number.isInteger(value) && !Number.isSafeInteger(value);
  if (Array.isArray(value)) return value.some(hasRoundedInteger);
  if (value && typeof value === "object") return Object.values(value).some(hasRoundedInteger);
  return false;
}
