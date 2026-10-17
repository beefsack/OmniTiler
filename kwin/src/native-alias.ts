// Offline-fake aliases, not KWin APIs: keep their names confined to this
// typed seam rather than adding them to the native API guard's allowlist.
// Callers prefer audited native properties, falling back on absence/type
// mismatch (strictly undefined-only in world-observe). The unchanged guard
// still rejects direct alias reads and unaudited native API literals.
export type NativeAliasName = "fullscreen" | "maximized" | "sticky";

export function readNativeAlias(
    readProp: (value: object, property: string) => unknown,
    ref: object,
    name: NativeAliasName,
): unknown {
    return readProp(ref, name);
}
